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

export function notebookWikiTargetAtOffset(text: string, offset: number) {
  return findNotebookWikiLinks(text).find((range) => offset >= range.from && offset <= range.to)?.target
}
