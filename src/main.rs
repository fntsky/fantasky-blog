mod auth;
mod migrations;
mod posts;

use std::env;
use std::error::Error;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

use crate::auth::{AuthError, current_user};
use crate::posts::{PostDetail, PostSummary};

#[derive(Clone)]
struct AppState {
    database: Arc<Mutex<Connection>>,
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct PostList {
    posts: Vec<PostSummary>,
}

#[derive(Serialize)]
struct PostResponse {
    post: PostDetail,
}

#[derive(Deserialize)]
struct NewPost {
    slug: String,
    title: String,
    date: String,
    excerpt: String,
    tags: Vec<String>,
    markdown: String,
}

#[derive(Deserialize)]
struct UpdatedPost {
    title: String,
    date: String,
    excerpt: String,
    tags: Vec<String>,
    markdown: String,
}

#[derive(Serialize)]
struct SessionResponse {
    username: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let host = env_string("BLOG_HOST", "127.0.0.1");
    let port = env_string("BLOG_PORT", "53213")
        .parse::<u16>()
        .map_err(|_| "BLOG_PORT must be a port number")?;
    let database_path = PathBuf::from(env_string("BLOG_DATABASE_PATH", "data/blog.sqlite3"));
    let content_directory = PathBuf::from(env_string("BLOG_CONTENT_DIR", "content"));
    let frontend_directory = PathBuf::from(env_string("BLOG_FRONTEND_DIR", "frontend/dist"));

    if let Some(parent) = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&content_directory)?;
    let mut database = Connection::open(&database_path)?;
    database.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrations::migrate(&mut database)?;
    posts::initialize(&mut database, &content_directory)?;

    let state = AppState {
        database: Arc::new(Mutex::new(database)),
    };
    let index_file = frontend_directory.join("index.html");
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/api/posts", get(list_posts))
        .route("/api/posts/{slug}", get(get_post))
        .route("/api/admin/session", get(admin_session))
        .route("/api/admin/posts", post(create_post).get(admin_list_posts))
        .route(
            "/api/admin/posts/{slug}",
            put(update_post).get(admin_get_post).delete(delete_post),
        )
        .route("/api/admin/posts/{slug}/visibility", put(set_visibility))
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .fallback_service(ServeDir::new(&frontend_directory).fallback(ServeFile::new(index_file)))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state);

    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = TcpListener::bind(address).await?;
    println!("RS Blog listening on http://{address}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn env_string(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

async fn health() -> &'static str {
    "ok"
}

async fn list_posts(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PostList>, StatusCode> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let database = state
        .database
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let posts = posts::list(&database, limit, false).map_err(|error| {
        eprintln!("Could not list blog posts: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(PostList { posts }))
}

async fn get_post(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<PostResponse>, StatusCode> {
    let database = state
        .database
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let post = posts::find(&database, &slug).map_err(|error| {
        eprintln!("Could not read blog post {slug:?}: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match post {
        Some(post) if !post.hidden => Ok(Json(PostResponse { post })),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn admin_list_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PostList>, ApiError> {
    current_user(&headers).map_err(ApiError::Auth)?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    let posts = posts::list(&database, i64::MAX as usize, true).map_err(|_| ApiError::Internal)?;
    Ok(Json(PostList { posts }))
}

async fn admin_get_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<PostResponse>, ApiError> {
    current_user(&headers).map_err(ApiError::Auth)?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    let post = posts::find(&database, &slug)
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(PostResponse { post }))
}

async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    current_user(&headers).map_err(ApiError::Auth)?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    if !posts::delete(&database, &slug).map_err(|_| ApiError::Internal)? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Visibility {
    hidden: bool,
}

async fn set_visibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<Visibility>,
) -> Result<Json<PostResponse>, ApiError> {
    let user = current_user(&headers).map_err(ApiError::Auth)?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    let post = posts::set_hidden(&database, &slug, input.hidden, &user.username)
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(PostResponse { post }))
}

async fn admin_session(headers: HeaderMap) -> Result<Json<SessionResponse>, AuthError> {
    let user = current_user(&headers)?;
    Ok(Json(SessionResponse {
        username: user.username,
    }))
}

async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NewPost>,
) -> Result<(StatusCode, Json<PostResponse>), ApiError> {
    let user = current_user(&headers).map_err(ApiError::Auth)?;
    validate_post(
        &input.slug,
        &input.title,
        &input.date,
        &input.excerpt,
        &input.tags,
        &input.markdown,
    )?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    let post = posts::create(
        &database,
        &input.slug,
        &input.title,
        &input.date,
        &input.excerpt,
        &input.tags,
        &input.markdown,
        &user.username,
    )
    .map_err(|error| {
        if matches!(&error, rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation)
        {
            ApiError::Conflict
        } else {
            eprintln!("Could not create blog post: {error}");
            ApiError::Internal
        }
    })?;
    Ok((StatusCode::CREATED, Json(PostResponse { post })))
}

async fn update_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<UpdatedPost>,
) -> Result<Json<PostResponse>, ApiError> {
    let user = current_user(&headers).map_err(ApiError::Auth)?;
    validate_post(
        &slug,
        &input.title,
        &input.date,
        &input.excerpt,
        &input.tags,
        &input.markdown,
    )?;
    let database = state.database.lock().map_err(|_| ApiError::Internal)?;
    let post = posts::update(
        &database,
        &slug,
        &input.title,
        &input.date,
        &input.excerpt,
        &input.tags,
        &input.markdown,
        &user.username,
    )
    .map_err(|error| {
        eprintln!("Could not update blog post {slug:?}: {error}");
        ApiError::Internal
    })?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(PostResponse { post }))
}

fn validate_post(
    slug: &str,
    title: &str,
    date: &str,
    excerpt: &str,
    tags: &[String],
    markdown: &str,
) -> Result<(), ApiError> {
    if slug.is_empty()
        || slug.len() > 80
        || slug.starts_with('-')
        || slug.ends_with('-')
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ApiError::Invalid(
            "slug 只能包含小写字母、数字和短横线，且不能以短横线开头或结尾。".into(),
        ));
    }
    if title.trim().is_empty() || title.chars().count() > 120 {
        return Err(ApiError::Invalid("标题不能为空且最多 120 个字符。".into()));
    }
    if !valid_date(date) {
        return Err(ApiError::Invalid("日期需使用 YYYY-MM-DD 格式。".into()));
    }
    if excerpt.chars().count() > 500 {
        return Err(ApiError::Invalid("摘要最多 500 个字符。".into()));
    }
    if tags.len() > 12
        || tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.chars().count() > 40)
    {
        return Err(ApiError::Invalid(
            "标签最多 12 个，每个标签最多 40 个字符。".into(),
        ));
    }
    if markdown.trim().is_empty() || markdown.len() > 1_500_000 {
        return Err(ApiError::Invalid(
            "Markdown 正文不能为空且最多 1.5 MB。".into(),
        ));
    }
    Ok(())
}

fn valid_date(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Debug)]
enum ApiError {
    Auth(AuthError),
    Invalid(String),
    Conflict,
    NotFound,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::Auth(AuthError::Unauthenticated) => {
                (StatusCode::UNAUTHORIZED, "请先登录 Formicary。".to_owned())
            }
            Self::Invalid(message) => (StatusCode::BAD_REQUEST, message),
            Self::Conflict => (StatusCode::CONFLICT, "这个 slug 已经被使用。".to_owned()),
            Self::NotFound => (StatusCode::NOT_FOUND, "文章不存在。".to_owned()),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "保存文章时发生错误。".to_owned(),
            ),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "API 端点不存在。"})),
    )
}
