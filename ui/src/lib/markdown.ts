import { marked, type RendererObject } from 'marked'
import { markedHighlight } from 'marked-highlight'
import hljs from 'highlight.js'
import DOMPurify from 'dompurify'

// Configure syntax highlighting
marked.use(
  markedHighlight({
    emptyLangClass: 'hljs',
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = hljs.getLanguage(lang) ? lang : 'plaintext'
      return hljs.highlight(code, { language }).value
    },
  })
)

// Open all links externally (handled by the global click handler in App.svelte)
const renderer: RendererObject = {
  link({ href, title, text }) {
    const titleAttr = title ? ` title="${title}"` : ''
    return `<a href="${href}" ${titleAttr} class="md-link" data-external="true" rel="noopener noreferrer">${text}</a>`
  },
}

marked.use({ renderer })

export function renderMarkdown(text: string): string {
  const html = marked.parse(text) as string
  return DOMPurify.sanitize(html, {
    ADD_ATTR: ['data-external', 'class'],
    FORCE_BODY: false,
  })
}

// Expanded config for card bodies — allows progress bars, checkboxes, labels, inline styles
export function renderCardMarkdown(text: string): string {
  const html = marked.parse(text) as string
  return DOMPurify.sanitize(html, {
    ADD_ATTR: ['data-external', 'class', 'type', 'min', 'max', 'value',
               'placeholder', 'checked', 'disabled', 'for', 'style'],
    ADD_TAGS: ['progress', 'meter', 'input', 'label'],
    FORCE_BODY: false,
  })
}
