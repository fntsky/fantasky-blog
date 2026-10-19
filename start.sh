#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary="$project_root/dist/rs_blog"

if [ ! -x "$binary" ]; then
    printf '%s\n' "RS Blog binary not found at $binary; run 'make build' first." >&2
    exit 1
fi

cd "$project_root"
export BLOG_HOST="${BLOG_HOST:-127.0.0.1}"
export BLOG_PORT="${BLOG_PORT:-53213}"
export BLOG_DATABASE_PATH="${BLOG_DATABASE_PATH:-$project_root/data/blog.sqlite3}"
export BLOG_CONTENT_DIR="${BLOG_CONTENT_DIR:-$project_root/content}"
export BLOG_FRONTEND_DIR="${BLOG_FRONTEND_DIR:-$project_root/frontend/dist}"

exec "$binary"
