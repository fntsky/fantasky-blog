use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub date: String,
    pub tags: Vec<String>,
    pub hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct PostDetail {
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub date: String,
    pub tags: Vec<String>,
    pub hidden: bool,
    pub markdown: String,
}

pub fn initialize(database: &mut Connection, directory: &Path) -> Result<(), Box<dyn Error>> {
    let imported: bool =
        database.query_row("SELECT imported FROM blog_state WHERE id = 1", [], |row| {
            row.get(0)
        })?;
    if !imported {
        let transaction = database.transaction()?;
        import_markdown_directory(&transaction, directory)?;
        transaction.execute("UPDATE blog_state SET imported = 1 WHERE id = 1", [])?;
        transaction.commit()?;
    }
    Ok(())
}

pub fn delete(database: &Connection, slug: &str) -> rusqlite::Result<bool> {
    Ok(database.execute("DELETE FROM posts WHERE slug = ?1", [slug])? > 0)
}

pub fn set_hidden(
    database: &Connection,
    slug: &str,
    hidden: bool,
    username: &str,
) -> rusqlite::Result<Option<PostDetail>> {
    database.execute(
        "UPDATE posts SET hidden = ?1, updated_by = ?2, updated_at = ?3 WHERE slug = ?4",
        params![hidden, username, unix_time() as i64, slug],
    )?;
    find(database, slug)
}

pub fn create(
    database: &Connection,
    slug: &str,
    title: &str,
    date: &str,
    excerpt: &str,
    tags: &[String],
    markdown: &str,
    username: &str,
) -> rusqlite::Result<PostDetail> {
    let tags_json = serde_json::to_string(tags).expect("tags should serialize as JSON");
    database.execute(
        "INSERT INTO posts (slug, title, excerpt, markdown, published_on, tags_json, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![slug, title.trim(), excerpt.trim(), markdown.trim(), date, tags_json, unix_time() as i64, username],
    )?;
    find(database, slug)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub fn update(
    database: &Connection,
    slug: &str,
    title: &str,
    date: &str,
    excerpt: &str,
    tags: &[String],
    markdown: &str,
    username: &str,
) -> rusqlite::Result<Option<PostDetail>> {
    let tags_json = serde_json::to_string(tags).expect("tags should serialize as JSON");
    let changed = database.execute(
        "UPDATE posts
         SET title = ?1, excerpt = ?2, markdown = ?3, published_on = ?4,
             tags_json = ?5, updated_at = ?6, updated_by = ?7
         WHERE slug = ?8",
        params![
            title.trim(),
            excerpt.trim(),
            markdown.trim(),
            date,
            tags_json,
            unix_time() as i64,
            username,
            slug
        ],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    find(database, slug)
}

struct MarkdownPost {
    slug: String,
    title: String,
    excerpt: String,
    date: String,
    tags: Vec<String>,
    markdown: String,
}

pub fn import_markdown_directory(
    database: &Connection,
    directory: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut files = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        })
        .collect::<Vec<PathBuf>>();
    files.sort();

    for path in files {
        let post = parse_markdown_post(&path)?;
        let tags_json = serde_json::to_string(&post.tags)?;
        database.execute(
            "INSERT INTO posts (slug, title, excerpt, markdown, published_on, tags_json, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'Markdown import', 'Markdown import')
             ON CONFLICT(slug) DO UPDATE SET
                 title = excluded.title,
                 excerpt = excluded.excerpt,
                 markdown = excluded.markdown,
                 published_on = excluded.published_on,
                 tags_json = excluded.tags_json,
                 updated_at = excluded.updated_at,
                 updated_by = excluded.updated_by",
            params![
                post.slug,
                post.title,
                post.excerpt,
                post.markdown,
                post.date,
                tags_json,
                unix_time() as i64
            ],
        )?;
    }
    Ok(())
}

pub fn list(
    database: &Connection,
    limit: usize,
    include_hidden: bool,
) -> rusqlite::Result<Vec<PostSummary>> {
    let mut statement = database.prepare(
        "SELECT slug, title, excerpt, published_on, tags_json, hidden
         FROM posts WHERE (hidden = 0 OR ?2) ORDER BY published_on DESC, slug ASC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64, include_hidden], |row| {
        let tags_json: String = row.get(4)?;
        Ok(PostSummary {
            slug: row.get(0)?,
            title: row.get(1)?,
            excerpt: row.get(2)?,
            date: row.get(3)?,
            hidden: row.get(5)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        })
    })?;
    rows.collect()
}

pub fn find(database: &Connection, slug: &str) -> rusqlite::Result<Option<PostDetail>> {
    database
        .query_row(
            "SELECT slug, title, excerpt, published_on, tags_json, markdown, hidden
             FROM posts WHERE slug = ?1",
            params![slug],
            |row| {
                let tags_json: String = row.get(4)?;
                Ok(PostDetail {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    excerpt: row.get(2)?,
                    date: row.get(3)?,
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                    markdown: row.get(5)?,
                    hidden: row.get(6)?,
                })
            },
        )
        .optional()
}

fn parse_markdown_post(path: &Path) -> Result<MarkdownPost, Box<dyn Error>> {
    let slug = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{}: Markdown file name is not valid UTF-8", path.display()))?
        .to_owned();
    if slug.is_empty()
        || !slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(format!(
            "{}: file name must use lowercase letters, digits, and hyphens",
            path.display()
        )
        .into());
    }

    let source = fs::read_to_string(path)?;
    let (metadata_source, markdown) = split_frontmatter(&source)
        .ok_or_else(|| format!("{}: expected --- front matter delimiters", path.display()))?;
    let mut metadata = HashMap::new();
    for line in metadata_source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("{}: invalid front matter line {line:?}", path.display()))?;
        metadata.insert(key.trim().to_owned(), value.trim().to_owned());
    }

    let title = metadata
        .get("title")
        .map(|value| unquote(value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{}: front matter needs a title", path.display()))?;
    let date = metadata
        .get("date")
        .map(|value| unquote(value))
        .filter(|value| valid_iso_date(value))
        .ok_or_else(|| format!("{}: date must use YYYY-MM-DD", path.display()))?;
    let excerpt = metadata
        .get("excerpt")
        .map(|value| unquote(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| derive_excerpt(markdown));
    let tags = metadata
        .get("tags")
        .map(|value| parse_tags(value))
        .unwrap_or_default();
    if markdown.trim().is_empty() {
        return Err(format!("{}: Markdown body must not be empty", path.display()).into());
    }

    Ok(MarkdownPost {
        slug,
        title,
        excerpt,
        date,
        tags,
        markdown: markdown.trim().to_owned(),
    })
}

fn split_frontmatter(source: &str) -> Option<(&str, &str)> {
    let source = source
        .strip_prefix("---\r\n")
        .or_else(|| source.strip_prefix("---\n"))?;
    if let Some((metadata, body)) = source.split_once("\r\n---\r\n") {
        Some((metadata, body))
    } else {
        source.split_once("\n---\n")
    }
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character| character == '"' || character == '\'')
        .to_owned()
}

fn valid_iso_date(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts
            .iter()
            .all(|part| part.chars().all(|character| character.is_ascii_digit()))
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(unquote)
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn derive_excerpt(markdown: &str) -> String {
    let paragraph = markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or_default()
        .trim_start_matches(['>', '-', '*', ' ']);
    let mut excerpt = paragraph.chars().take(180).collect::<String>();
    if paragraph.chars().count() > 180 {
        excerpt.push('…');
    }
    excerpt
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_deletion_and_initial_import_survive_restart() {
        let mut database = Connection::open_in_memory().unwrap();
        crate::migrations::migrate(&mut database).unwrap();
        initialize(&mut database, Path::new("content")).unwrap();
        let imported = list(&database, 100, true).unwrap();
        assert!(!imported.is_empty());
        let slug = &imported[0].slug;
        assert!(
            set_hidden(&database, slug, true, "admin")
                .unwrap()
                .unwrap()
                .hidden
        );
        assert!(
            !list(&database, 100, false)
                .unwrap()
                .iter()
                .any(|post| &post.slug == slug)
        );
        assert!(
            list(&database, 100, true)
                .unwrap()
                .iter()
                .any(|post| &post.slug == slug)
        );
        update(
            &database,
            slug,
            "Edited",
            "2026-10-25",
            "",
            &[],
            "Body",
            "admin",
        )
        .unwrap();
        assert!(find(&database, slug).unwrap().unwrap().hidden);
        assert!(
            !set_hidden(&database, slug, false, "admin")
                .unwrap()
                .unwrap()
                .hidden
        );
        for post in imported {
            assert!(delete(&database, &post.slug).unwrap());
            assert!(!delete(&database, &post.slug).unwrap());
            assert!(find(&database, &post.slug).unwrap().is_none());
        }
        crate::migrations::migrate(&mut database).unwrap();
        initialize(&mut database, Path::new("content")).unwrap();
        assert!(list(&database, 100, true).unwrap().is_empty());
        assert!(
            set_hidden(&database, "missing", true, "admin")
                .unwrap()
                .is_none()
        );
    }
}
