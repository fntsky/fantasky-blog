# RS Blog

一个供 Formicary Chamber 挂载的 Markdown 博客模块。后端使用 Rust、Axum 和 SQLite，前端使用 Vue 3。文章对访客公开阅读；写作与编辑复用 Formicary 的登录会话，不单独创建博客账号。

## Formicary 接入

Formicary 的正式模块配置位于 `formicary/chamber/rs_blog/chamber.toml`，项目根目录也有一份 `chamber.toml`。AI 辅助开发时，`make copy-to-chamber` 会把完整博客副本放到被忽略的 `formicary/tmp/chamber/rs_blog`；从 `formicary/tmp/` 启动并将 `FORMICARY_CHAMBERS_DIR` 指向 `tmp/chamber` 后，Chamber 会运行该副本并将其挂载到 `/blog`。

打开：<http://localhost:53212/blog/>

使用 Formicary 登录后，可通过导航栏进入文章管理，在 `/blog/admin/posts/new` 写作，在 `/blog/admin/posts/{slug}/edit` 编辑。管理列表支持隐藏、恢复公开和永久删除文章（删除前需确认）。隐藏文章仅能在登录后的管理页面读取和编辑，公开列表和详情均不可访问。未登录时公开文章仍可阅读，管理页面会引导用户前往 Formicary 登录页 `/ui`。

Formicary 在转发 `/blog` 请求前验证原有登录会话，并将已验证的用户名传给博客后端；博客仅对带有该可信身份的管理请求开放写入。浏览器不能通过自带同名请求头绕过 Formicary 的会话验证。

配置中的 upstream 使用 `127.0.0.1:53213`。如果部署时目录结构不同，请调整 Chamber 的 `working_directory`；若端口有冲突，同时修改 `BLOG_PORT` 和 upstream。

## 本地开发

在 `rs_blog` 根目录构建并启动：

```sh
make build
./start.sh
```

`make build` 将 Rust release 可执行文件复制到 `dist/rs_blog`，并把 Vue 构建到 `frontend/dist`；`start.sh` 只启动 `dist/rs_blog`。`make run` 会先构建，再调用同一个启动脚本。

要将完整博客模块复制到 Formicary 的开发目录 `tmp/chamber/rs_blog` 并由 Chamber 启动：

```sh
make copy-to-chamber
```

该目标会复制源码、前端产物和可执行文件，不复制 Git 元数据、Cargo/npm 缓存；如果开发副本目录里已有 SQLite 数据库，会保留现有数据。Chamber 使用副本目录作为工作目录并执行 `./start.sh`，所以 Formicary 无需调用 Cargo。复制完成后，从 `formicary/tmp/` 启动 Formicary，并将 `FORMICARY_CHAMBERS_DIR` 设为 `formicary/tmp/chamber`。

服务默认监听 `127.0.0.1:53213`，SQLite 数据库位于 `data/blog.sqlite3`，示例 Markdown 位于 `content/`。想在 Vite 开发服务器中使用管理功能时，需同时运行 Formicary；`frontend/vite.config.js` 会将 `/blog/api` 代理到 Formicary 的 `127.0.0.1:53212`，以便由 Formicary 验证登录会话。

环境变量：

| 名称 | 默认值 | 用途 |
| --- | --- | --- |
| `BLOG_HOST` | `127.0.0.1` | 服务监听地址 |
| `BLOG_PORT` | `53213` | 服务监听端口 |
| `BLOG_DATABASE_PATH` | `data/blog.sqlite3` | SQLite 数据库路径 |
| `BLOG_CONTENT_DIR` | `content` | Markdown 初始导入目录 |
| `BLOG_FRONTEND_DIR` | `frontend/dist` | Vue 构建产物目录 |

## 文章存储

首次初始化时，如果 SQLite 中还没有文章，服务会将 `content/` 内的 `.md` 文件导入数据库。后续 SQLite 是文章数据的权威来源，重启时不会用 Markdown 文件覆盖通过页面保存的内容。初始化完成后，即使删除全部文章，重启也不会重新导入示例内容。新文章和编辑内容会保存在 `data/blog.sqlite3`。

Markdown 文件使用 YAML 风格的简单 front matter：

```markdown
---
title: "一篇新文章"
date: 2026-10-19
excerpt: "列表页展示的简短摘要。"
tags: [Rust, 随笔]
---

# 一篇新文章

正文使用标准 Markdown。
```

页面的 Markdown 预览与阅读视图会禁用原始 HTML，并清理渲染结果。

## HTTP API

- `GET /api/posts`：公开文章列表，可用 `?limit=20` 限制数量（1–100）。
- `GET /api/posts/{slug}`：公开文章详情，包含 Markdown 正文。
- `GET /api/admin/session`：返回当前 Formicary 登录用户；未登录返回 `401`。
- `GET /api/admin/posts`：管理文章列表，包含隐藏文章，需要 Formicary 登录。
- `GET /api/admin/posts/{slug}`：管理文章详情，包含隐藏文章，需要 Formicary 登录。
- `PUT /api/admin/posts/{slug}/visibility`：设置可见性，JSON 为 `{"hidden": true}`（隐藏）或 `{"hidden": false}`（恢复公开），需要 Formicary 登录。
- `DELETE /api/admin/posts/{slug}`：永久删除文章，需要 Formicary 登录。
- `POST /api/admin/posts`：创建文章，需要 Formicary 登录。
- `PUT /api/admin/posts/{slug}`：编辑文章，需要 Formicary 登录。
- `GET /healthz`：服务健康状态。

通过 Formicary 访问时，前缀是 `/blog`，例如 `/blog/api/posts`。项目暂不提供注册、博客账号或独立登录功能。
