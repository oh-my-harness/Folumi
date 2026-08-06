export interface NotebookWikiLinkRange {
  from: number
  to: number
  target: string
}

export function findNotebookWikiLinks(text: string): NotebookWikiLinkRange[] {
  const ranges: NotebookWikiLinkRange[] = []
  const pattern = /\[\[([^\]\n]+)\]\]/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > 0 && text[match.index - 1] === '!') continue
    const target = match[1]?.split('|', 1)[0]?.trim()
    if (!target) continue
    ranges.push({ from: match.index, to: match.index + match[0].length, target })
  }
  return ranges
}

const WIKI_HREF_PREFIX = '#folumi-wiki-'
const EMBED_HREF_PREFIX = '#folumi-embed-'

export function prepareNotebookMarkdownForEditor(markdown: string) {
  return mapMarkdownProse(markdown, (text) => text.replace(/(!?)\[\[([^\]\n]+)\]\]/g, (_match, embed: string, inner: string) => {
    const encoded = encodeURIComponent(inner)
    if (embed) return `[${escapeMarkdownLabel(`![[${inner}]]`)}](${EMBED_HREF_PREFIX}${encoded})`
    const [rawTarget, rawAlias] = inner.split('|')
    const target = rawTarget?.trim()
    if (!target) return _match
    const label = rawAlias?.trim() || target
    return `[${escapeMarkdownLabel(label)}](${WIKI_HREF_PREFIX}${encoded})`
  }))
}

export function restoreNotebookMarkdownFromEditor(markdown: string) {
  return mapMarkdownProse(markdown, (text) => text.replace(
    /\[(?:\\.|[^\]])*\]\((#folumi-(?:wiki|embed)-)([^)\s]+)\)/g,
    (_match, prefix: string, encoded: string) => {
      try {
        const inner = decodeURIComponent(encoded)
        return prefix === EMBED_HREF_PREFIX ? `![[${inner}]]` : `[[${inner}]]`
      } catch {
        return _match
      }
    },
  ))
}

export function notebookWikiTargetFromHref(href: string) {
  if (!href.startsWith(WIKI_HREF_PREFIX)) return undefined
  try {
    return decodeURIComponent(href.slice(WIKI_HREF_PREFIX.length)).split('|', 1)[0]?.trim() || undefined
  } catch {
    return undefined
  }
}

function escapeMarkdownLabel(value: string) {
  return value.replace(/\\/g, '\\\\').replace(/\[/g, '\\[').replace(/\]/g, '\\]')
}

function mapMarkdownProse(markdown: string, transform: (text: string) => string) {
  const lines = markdown.match(/[^\n]*(?:\n|$)/g) ?? []
  let fence: { marker: string; length: number } | null = null
  return lines.map((line) => {
    const body = line.replace(/\r?\n$/, '')
    const ending = line.slice(body.length)
    const fenceMatch = body.trim().match(/^(`{3,}|~{3,})/)
    const marker = fenceMatch?.[1]
    if (fence) {
      if (marker && marker[0] === fence.marker && marker.length >= fence.length) fence = null
      return line
    }
    if (marker) {
      fence = { marker: marker[0]!, length: marker.length }
      return line
    }
    return `${mapInlineCode(body, transform)}${ending}`
  }).join('')
}

function mapInlineCode(line: string, transform: (text: string) => string) {
  let output = ''
  let cursor = 0
  const code = /(`+)(.+?)\1/g
  let match: RegExpExecArray | null
  while ((match = code.exec(line)) !== null) {
    output += transform(line.slice(cursor, match.index))
    output += match[0]
    cursor = match.index + match[0].length
  }
  return output + transform(line.slice(cursor))
}
