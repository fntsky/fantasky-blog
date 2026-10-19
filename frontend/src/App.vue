<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import DOMPurify from 'dompurify'
import MarkdownIt from 'markdown-it'
import { mapPosition } from './scroll-map'

const markdown = new MarkdownIt({ html: false, linkify: true, typographer: true })
// Code renderers do not include token attributes by default.
for (const type of ['fence', 'code_block']) {
  const render = markdown.renderer.rules[type]
  markdown.renderer.rules[type] = (tokens, index, options, env, renderer) => {
    const html = render(tokens, index, options, env, renderer)
    const token = tokens[index]
    return token.map ? html.replace('<pre', `<pre data-source-start="${token.map[0]}" data-source-end="${token.map[1]}"`) : html
  }
}
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
const selectedTag = ref('全部')
const availableTags = computed(() => [...new Set(posts.value.flatMap((item) => item.tags))])
const filteredPosts = computed(() => selectedTag.value === '全部' ? posts.value : posts.value.filter((item) => item.tags.includes(selectedTag.value)))
const post = ref(null)
const currentUser = ref(null)
const loading = ref(true)
const sessionLoading = ref(true)
const sessionError = ref(false)
const error = ref('')
const editorError = ref('')
const saving = ref(false)
const tagsText = ref('')
const editor = ref({ slug: '', title: '', date: '', excerpt: '', markdown: '' })
const articleContent = computed(() => {
  if (!post.value) return { html: '', headings: [] }
  const tokens = markdown.parse(post.value.markdown, {})
  const headings = []
  tokens.forEach((token, index) => {
    if (token.type !== 'heading_open') return
    const inline = tokens[index + 1]
    const title = (inline?.children || []).map((child) => child.type === 'image' ? child.content : child.type === 'text' || child.type === 'code_inline' ? child.content : child.type === 'softbreak' ? ' ' : '').join('') || inline?.content || ''
    const id = `section-${headings.length + 1}`
    token.attrSet('id', id)
    headings.push({ id, title, level: Number(token.tag.slice(1)) })
  })
  return { html: DOMPurify.sanitize(markdown.renderer.render(tokens, markdown.options, {})), headings }
})
const renderedMarkdown = computed(() => articleContent.value.html)
const wideStudio = ref(window.matchMedia('(min-width: 1700px)').matches)
const studioMedia = window.matchMedia('(min-width: 1700px)')
const updateStudioWidth = (event) => { wideStudio.value = event.matches }
onMounted(() => studioMedia.addEventListener('change', updateStudioWidth))
onUnmounted(() => studioMedia.removeEventListener('change', updateStudioWidth))
const markdownInput = ref(null)
const previewElement = ref(null)
const isWriting = computed(() => isAdminPage.value && editorMode.value !== 'list' && Boolean(currentUser.value) && !loading.value)
const renderedPreview = computed(() => {
  const tokens = markdown.parse(editor.value.markdown || '', {})
  tokens.forEach((token) => {
    if (token.map && token.type !== 'inline' && token.nesting !== -1) {
      token.attrSet('data-source-start', String(token.map[0]))
      token.attrSet('data-source-end', String(token.map[1]))
    }
  })
  return DOMPurify.sanitize(markdown.renderer.render(tokens, markdown.options, {}))
})
let sourceMeasurement = null
let syncFrame = 0
const expectedScroll = { source: null, preview: null }
onUnmounted(() => cancelAnimationFrame(syncFrame))
function measureSource(input) {
  const style = getComputedStyle(input)
  const key = `${input.clientWidth}/${style.font}/${style.lineHeight}/${style.padding}/${input.value}`
  if (sourceMeasurement?.key === key) return sourceMeasurement
  const mirror = document.createElement('div')
  for (const property of ['fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'lineHeight', 'letterSpacing', 'padding', 'boxSizing', 'tabSize', 'wordBreak', 'overflowWrap', 'textIndent']) mirror.style[property] = style[property]
  Object.assign(mirror.style, { position: 'fixed', visibility: 'hidden', pointerEvents: 'none', whiteSpace: 'pre-wrap', width: `${input.clientWidth}px`, top: '0', left: '0' })
  const lines = input.value.split('\n')
  const markers = lines.map((line, index) => {
    const marker = document.createElement('span')
    marker.textContent = '\u200b'
    mirror.append(marker, document.createTextNode(line + (index < lines.length - 1 ? '\n' : '')))
    return marker
  })
  document.body.append(mirror)
  const top = mirror.getBoundingClientRect().top
  const positions = markers.map((marker) => marker.getBoundingClientRect().top - top)
  mirror.remove()
  sourceMeasurement = { key, positions }
  return sourceMeasurement
}
function scrollPoints(input, pane, forScroll = false) {
  const { positions } = measureSource(input)
  const sourceMax = Math.max(0, input.scrollHeight - input.clientHeight)
  const previewMax = Math.max(0, pane.scrollHeight - pane.clientHeight)
  const paneTop = pane.getBoundingClientRect().top
  const points = [{ source: 0, preview: 0 }]
  for (const block of pane.querySelectorAll('[data-source-start]')) {
    const source = positions[Number(block.dataset.sourceStart)]
    const preview = block.getBoundingClientRect().top - paneTop + pane.scrollTop
    if (source === undefined) continue
    const point = { source: forScroll ? Math.min(sourceMax, source) : source, preview: forScroll ? Math.min(previewMax, preview) : preview }
    const previous = points.at(-1)
    if (point.source > previous.source && point.preview >= previous.preview && (!forScroll || point.source < sourceMax && point.preview < previewMax)) points.push(point)
  }
  points.push(forScroll ? { source: sourceMax, preview: previewMax } : { source: input.scrollHeight, preview: pane.scrollHeight })
  return points
}
function setSyncedScroll(side, element, value) {
  const target = Math.max(0, Math.min(element.scrollHeight - element.clientHeight, value))
  if (Math.abs(element.scrollTop - target) < 1) return
  expectedScroll[side] = target
  element.scrollTop = target
}
function syncScroll(side) {
  const element = side === 'source' ? markdownInput.value : previewElement.value
  if (!element) return
  const expected = expectedScroll[side]
  expectedScroll[side] = null
  if (expected !== null && Math.abs(element.scrollTop - expected) < 2) return
  cancelAnimationFrame(syncFrame)
  syncFrame = requestAnimationFrame(() => {
    const input = markdownInput.value
    const pane = previewElement.value
    if (!input || !pane) return
    const points = scrollPoints(input, pane, true)
    if (side === 'source') setSyncedScroll('preview', pane, mapPosition(points, input.scrollTop))
    else setSyncedScroll('source', input, mapPosition(points, pane.scrollTop, 'preview', 'source'))
  })
}
async function alignPreview() {
  await nextTick()
  const input = markdownInput.value
  const pane = previewElement.value
  if (!input || !pane) return
  cancelAnimationFrame(syncFrame)
  const line = input.value.slice(0, input.selectionStart).split('\n').length - 1
  const sourceY = measureSource(input).positions[line] || 0
  const viewportY = Math.max(0, Math.min(input.clientHeight, sourceY - input.scrollTop))
  setSyncedScroll('preview', pane, mapPosition(scrollPoints(input, pane), sourceY) - viewportY)
}
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
  <div class="site-shell" :class="{ 'writing-shell': isWriting }">
    <header class="topbar">
      <a class="brand" :href="isWriting ? `${basePath}/admin` : `${basePath}/`" aria-label="Fantasky`s Blog 首页">
        <span>Fantasky`s <span class="brand-light">Blog</span></span>
      </a>
      <div v-if="isWriting" class="studio-title"><p class="eyebrow">{{ editorMode === 'edit' ? 'EDIT ARTICLE' : 'NEW ARTICLE' }}</p><h1>{{ editorMode === 'edit' ? '编辑文章' : '写一篇新文章' }}</h1></div>
      <nav class="topnav" aria-label="主导航">
        <a :href="`${basePath}/`" :class="{ active: !isArticlePage && !isAdminPage }">文章</a>
        <template v-if="currentUser">
          <a :href="`${basePath}/admin`" :class="{ active: isAdminPage }">管理文章</a>
          <button v-if="isWriting" class="primary-button studio-publish" type="submit" form="article-editor" :disabled="saving">{{ saving ? '正在保存…' : editorMode === 'edit' ? '保存修改' : '发布文章' }} ↗</button>
          <a v-else class="nav-write" :href="`${basePath}/admin/posts/new`">写文章 <span>＋</span></a>
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

        <p v-if="editorError" class="form-error" role="alert">{{ editorError }}</p>
        <form id="article-editor" class="editor-form" @submit.prevent="savePost" @invalid.capture="($event) => { const settings = $event.target.closest('details'); if (settings) settings.open = true }">
          <div class="editor-main">
            <div class="writing-panes">
              <section class="writing-pane" aria-label="Markdown 编辑">
                <div class="markdown-toolbar"><span>MARKDOWN</span><span class="pane-label">编辑</span></div>
                <textarea ref="markdownInput" v-model="editor.markdown" @input="alignPreview" @click="alignPreview" @keyup="alignPreview" @select="alignPreview" @scroll="syncScroll('source')" class="markdown-editor" aria-label="文章正文（Markdown）" required maxlength="1500000" placeholder="# 从这里开始写作…&#10;&#10;支持标准 Markdown 语法。"></textarea>
              </section>
              <section class="writing-pane preview-pane" aria-label="文章实时预览">
                <div class="markdown-toolbar"><span>PREVIEW</span><span class="pane-label">实时预览</span></div>
                <div ref="previewElement" @scroll="syncScroll('preview')" @load.capture="alignPreview" class="markdown-preview markdown-body" v-html="renderedPreview"></div>
              </section>
            </div>
          </div>
          <details class="article-settings" :open="wideStudio"><summary>文章信息 <span>URL · 日期 · 摘要 · 标签</span></summary><aside class="editor-sidebar">
            <label class="field-label">文章标题<input v-model="editor.title" required maxlength="120" placeholder="给这篇文章一个标题" /></label>
            <div class="sidebar-title"><span>文章信息</span><span>01 — 04</span></div>
            <label v-if="editorMode === 'new'" class="field-label">URL 别名<input v-model="editor.slug" required maxlength="80" pattern="[a-z0-9]+([a-z0-9-]*[a-z0-9])?" placeholder="my-first-post" /><button class="text-action" type="button" @click="slugFromTitle">从英文标题生成</button></label>
            <label v-else class="field-label">URL 别名<input :value="editor.slug" disabled /></label>
            <label class="field-label">发布日期<input v-model="editor.date" type="date" required /></label>
            <label class="field-label">文章摘要<textarea v-model="editor.excerpt" maxlength="500" rows="3" placeholder="列表页展示的简短介绍"></textarea></label>
            <label class="field-label">标签<input v-model="tagsText" placeholder="Rust, 随笔, 工程" /><span class="field-hint">用逗号分隔，最多 12 个</span></label>

          </aside></details>
        </form>
      </section>
    </main>

    <main v-else-if="!isArticlePage" class="home-page">
      <section id="articles" class="articles-section">
        <div class="section-heading"><div><p class="eyebrow">THE JOURNAL</p><h1>最近文章 <span class="count">{{ String(posts.length).padStart(2, '0') }}</span></h1></div></div>
        <p v-if="loading" class="state-message">正在整理文章…</p>
        <p v-else-if="error" class="state-message error-message">{{ error }}</p>
        <div v-else-if="posts.length" class="journal-content">
          <div class="filter-bar" aria-label="按标签筛选文章"><button v-for="tag in ['全部', ...availableTags]" :key="tag" type="button" :class="{ selected: selectedTag === tag }" :aria-pressed="selectedTag === tag" @click="selectedTag = tag">{{ tag }}<span v-if="tag === '全部'"> / {{ posts.length }}</span></button></div>
          <div class="post-list">
          <article v-for="(item, index) in filteredPosts" :key="item.slug" class="post-card">
            <div class="post-number">{{ String(index + 1).padStart(2, '0') }}</div>
            <div class="post-main"><div class="post-meta"><time :datetime="item.date">{{ formatDate(item.date) }}</time><span class="meta-divider">/</span><span>阅读笔记</span></div><h3><a :href="articleUrl(item.slug)">{{ item.title }}<span class="title-arrow">↗</span></a></h3><p class="post-excerpt">{{ item.excerpt }}</p><div class="tag-list"><span v-for="tag in item.tags" :key="tag" class="tag">{{ tag }}</span></div></div>
            <a class="read-link" :href="articleUrl(item.slug)" :aria-label="`阅读：${item.title}`">阅读文章 <span>↗</span></a>
          </article>
          </div>
        </div>
        <div v-else class="empty-state"><span class="empty-mark">✳</span><p>还没有文章</p><span>通过 Formicary 登录后，可以创建第一篇文章。</span></div>
      </section>
    </main>

    <main v-else class="article-page">
      <a class="back-link" :href="`${basePath}/`"><span>←</span> 返回文章列表</a>
      <a v-if="currentUser && post" class="article-edit-link" :href="editUrl(post.slug)">编辑这篇文章 →</a>
      <p v-if="loading" class="state-message article-state">正在打开文章…</p>
      <p v-else-if="error" class="state-message error-message article-state">{{ error }}</p>
      <div v-else-if="post" class="article-with-toc" :class="{ 'has-toc': articleContent.headings.length }">
      <article class="article-layout">
        <header class="article-header"><div class="post-meta"><time :datetime="post.date">{{ formatDate(post.date) }}</time></div><h1>{{ post.title }}</h1><p class="article-excerpt">{{ post.excerpt }}</p><div class="tag-list"><span v-for="tag in post.tags" :key="tag" class="tag">{{ tag }}</span></div><div class="article-rule"><span></span><span></span><span></span></div></header>
        <div class="markdown-body" v-html="renderedMarkdown"></div>
        <footer class="article-footer"><span>—</span><p>感谢阅读</p><a :href="`${basePath}/`">继续浏览文章 <span>↗</span></a></footer>
      </article>
      <aside v-if="articleContent.headings.length" class="article-toc" aria-label="文章目录">
        <nav><h2>目录</h2><ol><li v-for="heading in articleContent.headings" :key="heading.id" :style="{ '--heading-indent': `${Math.min(heading.level - 1, 3) * 12}px` }"><a :href="`#${heading.id}`">{{ heading.title }}</a></li></ol></nav>
      </aside>
      </div>
    </main>

    <footer class="site-footer"><span class="footer-year">2026</span></footer>
  </div>
</template>
