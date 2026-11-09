// Remark plugin: replaces {{repo}}, {{releases}}, {{version}} and {{name}} in Markdown text and URLs,
// and prefixes root-relative links with the site's base path (Astro doesn't do that for Markdown links).
import { tokenValues } from '../site.config.mjs'

export function tokens({ base = '' } = {}) {
  const prefix = base && base !== '/' ? base.replace(/\/$/, '') : ''
  return (tree) => {
    const sub = (s) =>
      s.replace(/\{\{\s*([\w-]+)\s*\}\}/g, (m, key) => {
        const v = tokenValues[key]
        if (v === undefined) throw new Error(`Unknown token ${m}; add it to tokenValues in src/site.config.mjs`)
        return String(v)
      })
    const fixUrl = (u) => {
      u = sub(u)
      return prefix && u.startsWith('/') && !u.startsWith('//') && !u.startsWith(prefix + '/') ? prefix + u : u
    }
    const walk = (node) => {
      if ((node.type === 'text' || node.type === 'inlineCode' || node.type === 'code') && typeof node.value === 'string') node.value = sub(node.value)
      if ((node.type === 'link' || node.type === 'definition') && node.url) node.url = fixUrl(node.url)
      if (node.type === 'image' && node.url) node.url = sub(node.url)
      for (const c of node.children || []) walk(c)
    }
    walk(tree)
  }
}
