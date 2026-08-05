export interface NotebookVaultInfo {
  root: string
  external: boolean
  entries: number
}

export interface SaveToNotebookResult {
  entryId: string
  title: string
  path: string
}

export type GeneratedNotebookEntryType = 'chat_excerpt'

export interface NotebookFolderNode {
  name: string
  path: string
  children: NotebookFolderNode[]
}

export const LAST_NOTEBOOK_SAVE_FOLDER_KEY = 'llm-tutor:last-notebook-save-folder'

export function resolveGeneratedNotebookEntryType(
  requested?: GeneratedNotebookEntryType,
): GeneratedNotebookEntryType {
  return requested ?? 'chat_excerpt'
}

export function titleFromMarkdown(markdown: string) {
  const heading = markdown
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.startsWith('# '))
  if (heading) return heading.replace(/^#\s+/, '').trim().slice(0, 80) || 'Conversation Note'
  const first = markdown.trim().split('\n').find((line) => line.trim())
  return first?.trim().slice(0, 80) || 'Conversation Note'
}

export function normalizeNotebookFolderPath(path: string) {
  return path
    .replace(/\\/g, '/')
    .split('/')
    .map((part) => sanitizeNotebookPathPart(part))
    .filter(Boolean)
    .join('/')
}

export function notebookFileNameFromTitle(title: string) {
  return `${sanitizeNotebookPathPart(title) || 'Conversation Note'}.md`
}

export function normalizeNotebookFileName(value: string) {
  const withoutExtension = value.replace(/\.md$/i, '')
  const stem = sanitizeNotebookPathPart(withoutExtension)
  return stem ? `${stem}.md` : ''
}

export function notebookPath(folderPath: string, fileName: string) {
  const folder = normalizeNotebookFolderPath(folderPath)
  const file = normalizeNotebookFileName(fileName)
  if (!file) return ''
  return folder ? `${folder}/${file}` : file
}

export function normalizeNotebookEntryPath(path: string) {
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean)
  const fileName = parts.pop() ?? ''
  return notebookPath(parts.join('/'), fileName)
}

export function buildNotebookFolderTree(folders: string[]) {
  const roots: NotebookFolderNode[] = []
  const byPath = new Map<string, NotebookFolderNode>()

  for (const folder of folders.map(normalizeNotebookFolderPath).filter(Boolean).sort(comparePaths)) {
    let parentPath = ''
    for (const name of folder.split('/')) {
      const path = parentPath ? `${parentPath}/${name}` : name
      if (!byPath.has(path)) {
        const node = { name, path, children: [] as NotebookFolderNode[] }
        byPath.set(path, node)
        if (parentPath) byPath.get(parentPath)?.children.push(node)
        else roots.push(node)
      }
      parentPath = path
    }
  }

  return roots
}

export function notebookPathExists(path: string, entryPaths: string[]) {
  const target = normalizeComparableNotebookPath(path)
  return entryPaths.some((entryPath) => normalizeComparableNotebookPath(entryPath) === target)
}

export function relativeNotebookPath(vaultRoot: string, selectedPath: string) {
  const root = normalizeSystemPath(vaultRoot)
  const selected = normalizeSystemPath(selectedPath)
  const windowsPath = /^[a-zA-Z]:\//.test(root) || /^[a-zA-Z]:\//.test(selected)
  const comparableRoot = windowsPath ? root.toLowerCase() : root
  const comparableSelected = windowsPath ? selected.toLowerCase() : selected
  const prefix = comparableRoot === '/' ? '/' : `${comparableRoot}/`

  if (!root || !selected || !comparableSelected.startsWith(prefix)) {
    throw new Error('请选择当前 Notebook Vault 内的 Markdown 文件。')
  }
  const relative = selected.slice(root === '/' ? 1 : root.length + 1)
  if (!/\.md$/i.test(relative)) {
    throw new Error('Notebook 笔记必须使用 .md 文件扩展名。')
  }
  return relative.replace(/\\/g, '/')
}

export function desktopDefaultSavePath(vaultRoot: string, folderPath: string, fileName: string) {
  const separator = vaultRoot.includes('\\') ? '\\' : '/'
  const relativeParts = [
    ...normalizeNotebookFolderPath(folderPath).split('/').filter(Boolean),
    normalizeNotebookFileName(fileName),
  ].filter(Boolean)
  const root = vaultRoot.replace(/[\\/]+$/, '')
  if (!root && separator === '/') return `/${relativeParts.join('/')}`
  return [root, ...relativeParts].filter(Boolean).join(separator)
}

export function loadLastNotebookSaveFolder(folders: string[]) {
  try {
    const saved = normalizeNotebookFolderPath(localStorage.getItem(LAST_NOTEBOOK_SAVE_FOLDER_KEY) ?? '')
    return saved && folders.includes(saved) ? saved : ''
  } catch {
    return ''
  }
}

export function saveLastNotebookSaveFolder(folder: string) {
  try {
    localStorage.setItem(LAST_NOTEBOOK_SAVE_FOLDER_KEY, normalizeNotebookFolderPath(folder))
  } catch {
    // Storage may be unavailable in hardened webviews.
  }
}

export function folderFromNotebookPath(path: string) {
  const parts = path.replace(/\\/g, '/').split('/')
  parts.pop()
  return parts.join('/')
}

export function notebookFolderAncestors(path: string) {
  const ancestors = new Set<string>()
  let current = ''
  for (const part of normalizeNotebookFolderPath(path).split('/').filter(Boolean)) {
    current = current ? `${current}/${part}` : part
    ancestors.add(current)
  }
  return ancestors
}

function sanitizeNotebookPathPart(value: string) {
  return value
    .trim()
    .replace(/[<>:"|?*\x00-\x1F]/g, '')
    .replace(/[./\\]+$/g, '')
    .slice(0, 80)
}

function normalizeComparableNotebookPath(path: string) {
  return path.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '').toLowerCase()
}

function normalizeSystemPath(path: string) {
  const normalized = path.replace(/\\/g, '/').replace(/\/+$/g, '')
  return normalized || (path.startsWith('/') ? '/' : '')
}

function comparePaths(left: string, right: string) {
  return left.localeCompare(right, undefined, { sensitivity: 'base' })
}
