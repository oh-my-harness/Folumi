export interface NotebookEditorDocument {
  frontMatter: string
  body: string
}

export function splitNotebookEditorDocument(markdown: string): NotebookEditorDocument {
  const match = markdown.match(/^(?:\uFEFF)?---[\t ]*\r?\n[\s\S]*?\r?\n---[\t ]*(?:\r?\n|$)/)
  if (!match) return { frontMatter: '', body: markdown }
  return {
    frontMatter: match[0],
    body: markdown.slice(match[0].length),
  }
}

export function joinNotebookEditorDocument(frontMatter: string, body: string) {
  return `${frontMatter}${body}`
}
