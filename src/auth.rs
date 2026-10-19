use axum::Json;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone)]
pub enum AuthError {
    Unauthenticated,
}

#[derive(Debug)]
pub struct User {
    pub username: String,
}

pub fn current_user(headers: &HeaderMap) -> Result<User, AuthError> {
    let username = headers
        .get("x-formicary-user")
        .and_then(|value| value.to_str().ok())
        .filter(|username| !username.trim().is_empty())
        .ok_or(AuthError::Unauthenticated)?;
    Ok(User {
        username: username.to_owned(),
    })
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "请先登录 Formicary。"})),
            )
                .into_response(),
        }
    }
}
