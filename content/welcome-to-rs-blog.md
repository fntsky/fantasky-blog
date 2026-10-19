---
title: "欢迎来到 RS Blog"
date: 2026-10-19
excerpt: "一个用 Rust、Vue 和 SQLite 搭起的轻量 Markdown 阅读空间。"
tags: [Rust, Vue, Markdown]
---

# 欢迎来到 RS Blog

这里是一个安静的地方，用来记录工程实践、学习笔记和那些值得回头看的想法。

## 先把阅读做好

当前版本专注于 Markdown 阅读：文章从本地 Markdown 文件导入 SQLite，再由只读 API 提供给 Vue 页面。没有账户系统，也没有网页编辑入口。

```rust
fn keep_it_simple() -> &'static str {
    "write, publish, read"
}
```

新增文章只需在 `content/` 中放入一个带有标题和日期的 `.md` 文件。重新启动服务后，文章会进入数据库并出现在列表中。
