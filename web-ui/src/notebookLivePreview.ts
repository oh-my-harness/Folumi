export interface NotebookMarkdownBlock {
  start: number
  end: number
  source: string
  separator: string
}

interface MarkdownLine {
  start: number
  end: number
  body: string
}

export function splitNotebookMarkdownBlocks(markdown: string): NotebookMarkdownBlock[] {
  if (!markdown) return [{ start: 0, end: 0, source: '', separator: '' }]
  const lines = markdownLines(markdown)
  const blocks: NotebookMarkdownBlock[] = []
  let blockStart = 0
  let fence: { marker: string; length: number } | null = null

  const pushBlock = (start: number, end: number, separatorEnd = end) => {
    if (end <= start) {
      if (separatorEnd <= end) return
      const previous = blocks[blocks.length - 1]
      if (previous && previous.end + previous.separator.length === start) {
        previous.separator += markdown.slice(end, separatorEnd)
        return
      }
    }
    blocks.push({
      start,
      end,
      source: markdown.slice(start, end),
      separator: markdown.slice(end, separatorEnd),
    })
  }

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]!
    const trimmed = line.body.trim()
    const fenceMatch = trimmed.match(/^(`{3,}|~{3,})/)
    const marker = fenceMatch?.[1]

    if (!fence && marker) {
      fence = { marker: marker[0]!, length: marker.length }
      continue
    }
    if (fence && marker && marker[0] === fence.marker && marker.length >= fence.length) {
      fence = null
      continue
    }
    if (fence) continue

    if (/^#{1,6}(?:\s|$)/.test(trimmed)) {
      if (line.start > blockStart) pushBlock(blockStart, line.start)
      const sourceEnd = line.end - lineBreakLength(markdown.slice(line.start, line.end))
      pushBlock(line.start, sourceEnd, line.end)
      blockStart = line.end
      continue
    }

    if (!trimmed) {
      let separatorEnd = line.end
      while (index + 1 < lines.length && !lines[index + 1]!.body.trim()) {
        index += 1
        separatorEnd = lines[index]!.end
      }
      pushBlock(blockStart, line.start, separatorEnd)
      blockStart = separatorEnd
    }
  }

  if (blockStart < markdown.length) pushBlock(blockStart, markdown.length)
  return blocks.length > 0 ? blocks : [{ start: 0, end: markdown.length, source: markdown, separator: '' }]
}

export function replaceNotebookMarkdownBlock(
  markdown: string,
  start: number,
  end: number,
  source: string,
) {
  return `${markdown.slice(0, start)}${source}${markdown.slice(end)}`
}

function markdownLines(markdown: string): MarkdownLine[] {
  const lines: MarkdownLine[] = []
  let start = 0
  while (start < markdown.length) {
    const newline = markdown.indexOf('\n', start)
    const end = newline === -1 ? markdown.length : newline + 1
    const raw = markdown.slice(start, end)
    lines.push({ start, end, body: raw.replace(/\r?\n$/, '') })
    start = end
  }
  return lines
}

function lineBreakLength(value: string) {
  if (value.endsWith('\r\n')) return 2
  return value.endsWith('\n') ? 1 : 0
}
