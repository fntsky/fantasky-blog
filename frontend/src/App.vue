<script setup>
import { computed, onMounted, ref } from 'vue'
import DOMPurify from 'dompurify'
import MarkdownIt from 'markdown-it'

const markdown = new MarkdownIt({ html: false, linkify: true, typographer: true })
const basePath = import.meta.env.BASE_URL.replace(/\/$/, '')
const pathname = window.location.pathname.replace(/\/$/, '') || basePath
const localPath = pathname.startsWith(basePath) ? pathname.slice(basePath.length) : ''
const articleMatch = localPath.match(/^\/posts\/(.+)$/)
const editMatch = localPath.match(/^\/admin\/posts\/([^/]+)\/edit$/)
const slug = articleMatch ? decodeURIComponent(articleMatch[1]) : ''
const editSlug = editMatch ? decodeURIComponent(editMatch[1]) : ''
const isArticlePage = computed(() => Boolean(slug))
const isAdminPage = computed(() => localPath === '/admin' || localPath === '/admin/posts/new' || Boolean(editSlug))
const editorMode = computed(() => editSlug ? 'edit' : localPath === '/admin/posts/new' ? 'new' : 'list')

const posts = ref([])
const post = ref(null)
const currentUser = ref(null)
const loading = ref(true)
const sessionLoading = ref(true)
const sessionError = ref(false)
const error = ref('')
const editorError = ref('')
const saving = ref(false)
const preview = ref(false)
const tagsText = ref('')
const editor = ref({ slug: '', title: '', date: '', excerpt: '', markdown: '' })
const renderedMarkdown = computed(() =>
  post.value ? DOMPurify.sanitize(markdown.render(post.value.markdown)) : '',
)
const renderedPreview = computed(() => DOMPurify.sanitize(markdown.render(editor.value.markdown || '')))
const articleUrl = (postSlug) => `${basePath}/posts/${encodeURIComponent(postSlug)}`
const editUrl = (postSlug) => `${basePath}/admin/posts/${encodeURIComponent(postSlug)}/edit`
const formatDate = (date) => {
  const parsed = new Date(`${date}T00:00:00`)
  if (Number.isNaN(parsed.getTime())) return date
  return new Intl.DateTimeFormat('zh-CN', { year: 'numeric', month: 'long', day: 'numeric' }).format(parsed)
}
const today = () => {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
}

async function loadSession() {
  try {
    const response = await fetch(`${basePath}/api/admin/session`, { credentials: 'same-origin' })
    if (response.ok) currentUser.value = await response.json()
    else if (response.status !== 401) sessionError.value = true
  } catch {
    sessionError.value = true
  } finally {
    sessionLoading.value = false
  }
}

async function readResponse(response) {
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(data.error || '请求失败，请稍后重试。')
  return data
}

async function loadAdminPage() {
  if (!currentUser.value) {
    editorError.value = sessionError.value
      ? '暂时无法连接 Formicary，请确认主服务正在运行。'
      : '请先使用 Formicary 账号登录，再回来管理文章。'
    loading.value = false
    return
  }

  try {
    if (editorMode.value === 'list') {
      const data = await readResponse(await fetch(`${basePath}/api/posts`))
      posts.value = data.posts
    } else if (editorMode.value === 'edit') {
      const data = await readResponse(await fetch(`${basePath}/api/posts/${encodeURIComponent(editSlug)}`))
      post.value = data.post
      editor.value = {
        slug: data.post.slug,
        title: data.post.title,
        date: data.post.date,
        excerpt: data.post.excerpt,
        markdown: data.post.markdown,
      }
      tagsText.value = data.post.tags.join(', ')
    } else {
      editor.value = { slug: '', title: '', date: today(), excerpt: '', markdown: '' }
    }
  } catch (loadError) {
    editorError.value = loadError.message || '无法加载文章。'
  } finally {
    loading.value = false
  }
}

async function savePost() {
  editorError.value = ''
  saving.value = true
  const tags = tagsText.value.split(',').map((tag) => tag.trim()).filter(Boolean)
  const isEditing = editorMode.value === 'edit'
  const payload = {
    ...(isEditing ? {} : { slug: editor.value.slug.trim().toLowerCase() }),
    title: editor.value.title,
    date: editor.value.date,
    excerpt: editor.value.excerpt,
    tags,
    markdown: editor.value.markdown,
  }
  try {
    const response = await fetch(
      isEditing
        ? `${basePath}/api/admin/posts/${encodeURIComponent(editSlug)}`
        : `${basePath}/api/admin/posts`,
      {
        method: isEditing ? 'PUT' : 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
    )
    const data = await readResponse(response)
    window.location.assign(articleUrl(data.post.slug))
  } catch (saveError) {
    editorError.value = saveError.message || '保存失败，请稍后重试。'
  } finally {
    saving.value = false
  }
}

function slugFromTitle() {
  editor.value.slug = editor.value.title
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}

onMounted(async () => {
  const sessionTask = loadSession()
  if (isAdminPage.value) {
    await sessionTask
    await loadAdminPage()
    return
  }

  try {
    if (isArticlePage.value) {
      const data = await readResponse(await fetch(`${basePath}/api/posts/${encodeURIComponent(slug)}`))
      post.value = data.post
      document.title = `${post.value.title} · RS Blog`
    } else {
      const data = await readResponse(await fetch(`${basePath}/api/posts`))
      posts.value = data.posts
    }
  } catch (loadError) {
    error.value = loadError.message || '内容加载失败，请稍后重试。'
  } finally {
    loading.value = false
  }
  await sessionTask
})
</script>

<template>
  <div class="site-shell">
    <header class="topbar">
      <a class="brand" :href="`${basePath}/`" aria-label="RS Blog 首页">
        <span class="brand-mark">R</span>
        <span>RS <span class="brand-light">BLOG</span></span>
      </a>
      <nav class="topnav" aria-label="主导航">
        <a :href="`${basePath}/`" :class="{ active: !isArticlePage && !isAdminPage }">文章</a>
        <template v-if="currentUser">
          <a :href="`${basePath}/admin`" :class="{ active: isAdminPage }">管理文章</a>
          <a class="nav-write" :href="`${basePath}/admin/posts/new`">写文章 <span>＋</span></a>
          <span class="nav-note"><span class="status-dot"></span>{{ currentUser.username }}</span>
        </template>
        <a v-else-if="!sessionLoading" class="login-link" href="/ui">Formicary 登录 ↗</a>
        <span v-else class="nav-note"><span class="status-dot"></span>只读 · Markdown</span>
      </nav>
    </header>

    <main v-if="isAdminPage" class="admin-page">
      <a class="back-link" :href="`${basePath}/`"><span>←</span> 返回博客</a>
      <div v-if="loading" class="state-message admin-state">正在加载…</div>
      <section v-else-if="!currentUser" class="login-required">
        <span class="login-symbol">↗</span>
        <p class="eyebrow">FORMICARY ACCOUNT</p>
        <h1>登录后管理文章</h1>
        <p>{{ editorError }}</p>
        <a class="primary-button" href="/ui">前往 Formicary 登录 <span>↗</span></a>
      </section>
      <template v-else-if="editorMode === 'list'">
        <div class="admin-heading">
          <div><p class="eyebrow">BLOG STUDIO</p><h1>文章管理</h1><p>你好，{{ currentUser.username }}。从这里继续写作。</p></div>
          <a class="primary-button" :href="`${basePath}/admin/posts/new`">新建文章 <span>＋</span></a>
        </div>
        <p v-if="editorError" class="form-error">{{ editorError }}</p>
        <div v-else-if="posts.length" class="manage-list">
          <article v-for="item in posts" :key="item.slug" class="manage-row">
            <div class="manage-date">{{ formatDate(item.date) }}</div>
            <div class="manage-copy"><h2>{{ item.title }}</h2><p>{{ item.excerpt }}</p></div>
            <div class="manage-actions"><a :href="articleUrl(item.slug)">查看 ↗</a><a :href="editUrl(item.slug)">编辑 →</a></div>
          </article>
        </div>
        <div v-else class="empty-state"><span class="empty-mark">✳</span><p>还没有文章</p><a :href="`${basePath}/admin/posts/new`">写下第一篇文章 →</a></div>
      </template>
      <section v-else class="editor-page">
        <div class="editor-heading">
          <div><p class="eyebrow">{{ editorMode === 'edit' ? 'EDIT ARTICLE' : 'NEW ARTICLE' }}</p><h1>{{ editorMode === 'edit' ? '编辑文章' : '写一篇新文章' }}</h1></div>
          <a class="cancel-link" :href="`${basePath}/admin`">取消并返回</a>
        </div>
        <p v-if="editorError" class="form-error" role="alert">{{ editorError }}</p>
        <form class="editor-form" @submit.prevent="savePost">
          <div class="editor-main">
            <label class="field-label">文章标题<input v-model="editor.title" required maxlength="120" placeholder="给这篇文章一个标题" /></label>
            <div class="markdown-toolbar"><span>MARKDOWN BODY</span><button type="button" :class="{ selected: !preview }" @click="preview = false">编辑</button><button type="button" :class="{ selected: preview }" @click="preview = true">预览</button></div>
            <textarea v-if="!preview" v-model="editor.markdown" class="markdown-editor" required maxlength="1500000" placeholder="# 从这里开始写作…&#10;&#10;支持标准 Markdown 语法。"></textarea>
            <div v-else class="markdown-preview markdown-body" v-html="renderedPreview"></div>
            <p class="editor-help">支持 Markdown；原始 HTML 不会被渲染。建议先预览再保存。</p>
          </div>
          <aside class="editor-sidebar">
            <div class="sidebar-title"><span>文章信息</span><span>01 — 04</span></div>
            <label v-if="editorMode === 'new'" class="field-label">URL 别名<input v-model="editor.slug" required maxlength="80" pattern="[a-z0-9]+([a-z0-9-]*[a-z0-9])?" placeholder="my-first-post" /><button class="text-action" type="button" @click="slugFromTitle">从英文标题生成</button></label>
            <label v-else class="field-label">URL 别名<input :value="editor.slug" disabled /></label>
            <label class="field-label">发布日期<input v-model="editor.date" type="date" required /></label>
            <label class="field-label">文章摘要<textarea v-model="editor.excerpt" maxlength="500" rows="3" placeholder="列表页展示的简短介绍"></textarea></label>
            <label class="field-label">标签<input v-model="tagsText" placeholder="Rust, 随笔, 工程" /><span class="field-hint">用逗号分隔，最多 12 个</span></label>
            <button class="primary-button save-button" type="submit" :disabled="saving">{{ saving ? '正在保存…' : editorMode === 'edit' ? '保存修改' : '发布文章' }} <span>↗</span></button>
            <p class="save-note">保存后会立即公开显示在博客中。</p>
          </aside>
        </form>
      </section>
    </main>

    <main v-else-if="!isArticlePage" class="home-page">
      <section class="hero">
        <div class="hero-copy">
          <p class="eyebrow"><span class="eyebrow-line"></span> NOTES &amp; IDEAS</p>
          <h1>写点代码，<br /><span>也写下思考。</span></h1>
          <p class="hero-description">关于工程、学习，以及构建事物时遇到的真实问题。<br class="desktop-break" />用 Markdown 书写，在这里慢慢阅读。</p>
          <a class="scroll-link" href="#articles"><span class="scroll-arrow">↓</span> 向下阅读</a>
        </div>
        <div class="hero-art" aria-hidden="true">
          <div class="orbit orbit-one"></div><div class="orbit orbit-two"></div><div class="orbit orbit-three"></div>
          <div class="art-center"><span>读</span></div>
          <span class="art-dot dot-one"></span><span class="art-dot dot-two"></span><span class="art-dot dot-three"></span>
          <span class="art-caption">A PLACE TO THINK<br />AND MAKE</span>
        </div>
        <div class="hero-index"><span>01</span><span class="index-line"></span><span>阅读空间</span></div>
      </section>
      <section id="articles" class="articles-section">
        <div class="section-heading"><div><p class="eyebrow">THE JOURNAL</p><h2>最近文章 <span class="count">{{ String(posts.length).padStart(2, '0') }}</span></h2></div><p class="section-note">把值得回看的东西，<br />留在这里。</p></div>
        <p v-if="loading" class="state-message">正在整理文章…</p>
        <p v-else-if="error" class="state-message error-message">{{ error }}</p>
        <div v-else-if="posts.length" class="post-list">
          <article v-for="(item, index) in posts" :key="item.slug" class="post-card">
            <div class="post-number">{{ String(index + 1).padStart(2, '0') }}</div>
            <div class="post-main"><div class="post-meta"><time :datetime="item.date">{{ formatDate(item.date) }}</time><span class="meta-divider">/</span><span>阅读笔记</span></div><h3><a :href="articleUrl(item.slug)">{{ item.title }}<span class="title-arrow">↗</span></a></h3><p class="post-excerpt">{{ item.excerpt }}</p><div class="tag-list"><span v-for="tag in item.tags" :key="tag" class="tag">{{ tag }}</span></div></div>
            <a class="read-link" :href="articleUrl(item.slug)" :aria-label="`阅读：${item.title}`">阅读文章 <span>↗</span></a>
          </article>
        </div>
        <div v-else class="empty-state"><span class="empty-mark">✳</span><p>还没有文章</p><span>通过 Formicary 登录后，可以创建第一篇文章。</span></div>
      </section>
      <section class="about-strip"><span class="about-icon">✳</span><p>用 <strong>Rust</strong>、<strong>Vue</strong> 和 <strong>SQLite</strong> 搭建。文章管理复用 Formicary 登录。</p><span class="about-label">BUILT FOR READING</span></section>
    </main>

    <main v-else class="article-page">
      <a class="back-link" :href="`${basePath}/`"><span>←</span> 返回文章列表</a>
      <a v-if="currentUser && post" class="article-edit-link" :href="editUrl(post.slug)">编辑这篇文章 →</a>
      <p v-if="loading" class="state-message article-state">正在打开文章…</p>
      <p v-else-if="error" class="state-message error-message article-state">{{ error }}</p>
      <article v-else-if="post" class="article-layout">
        <header class="article-header"><div class="post-meta"><time :datetime="post.date">{{ formatDate(post.date) }}</time><span class="meta-divider">/</span><span>阅读笔记</span></div><h1>{{ post.title }}</h1><p class="article-excerpt">{{ post.excerpt }}</p><div class="tag-list"><span v-for="tag in post.tags" :key="tag" class="tag">{{ tag }}</span></div><div class="article-rule"><span></span><span></span><span></span></div></header>
        <div class="markdown-body" v-html="renderedMarkdown"></div>
        <footer class="article-footer"><span>—</span><p>感谢阅读</p><a :href="`${basePath}/`">继续浏览文章 <span>↗</span></a></footer>
      </article>
    </main>

    <footer class="site-footer"><a class="footer-brand" :href="`${basePath}/`">RS <span>BLOG</span></a><span>用心记录，安静阅读。</span><span class="footer-year">© 2026</span></footer>
  </div>
</template>
