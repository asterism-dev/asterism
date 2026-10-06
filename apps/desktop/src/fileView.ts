import hljs from 'highlight.js/lib/common';

const HIGHLIGHT_LIMIT = 200 * 1024;
const ENTITIES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };

export function languageFor(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase();
  const dot = name.lastIndexOf('.');
  const candidate = dot > 0 ? name.slice(dot + 1) : name;
  return hljs.getLanguage(candidate) ? candidate : null;
}

export function escapeHtml(text: string): string {
  return text.replace(/[&<>]/g, (c) => ENTITIES[c]);
}

/** HTML for `v-html`: hljs escapes its input, everything else goes through `escapeHtml`. */
export function renderCode(path: string, content: string): string {
  const language = content.length <= HIGHLIGHT_LIMIT ? languageFor(path) : null;
  return language ? hljs.highlight(content, { language }).value : escapeHtml(content);
}
