import { useMemo, useState } from 'react'
import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderPlus,
} from 'lucide-react'
import {
  buildNotebookFolderTree,
  notebookFolderAncestors,
  notebookPath,
  notebookPathExists,
  normalizeNotebookFolderPath,
} from '../notebookSave'
import type { NotebookFolderNode, SaveToNotebookResult } from '../notebookSave'

export function SaveNotebookDialog({
  folders,
  entryPaths,
  selectedFolder,
  newFolder,
  fileName,
  busy,
  error,
  onSelectedFolderChange,
  onNewFolderChange,
  onFileNameChange,
  onCancel,
  onSave,
}: {
  folders: string[]
  entryPaths: string[]
  selectedFolder: string
  newFolder: string
  fileName: string
  busy: boolean
  error: string
  onSelectedFolderChange: (value: string) => void
  onNewFolderChange: (value: string) => void
  onFileNameChange: (value: string) => void
  onCancel: () => void
  onSave: () => void
}) {
  const [expanded, setExpanded] = useState<Set<string>>(() => notebookFolderAncestors(selectedFolder))
  const [creatingFolder, setCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const effectiveFolder = newFolder || selectedFolder
  const tree = useMemo(
    () => buildNotebookFolderTree(newFolder ? [...folders, newFolder] : folders),
    [folders, newFolder],
  )
  const finalPath = notebookPath(effectiveFolder, fileName)
  const conflict = Boolean(finalPath) && notebookPathExists(finalPath, entryPaths)
  const normalizedNewFolderName = normalizeNotebookFolderPath(newFolderName)
  const canCreateFolder = Boolean(normalizedNewFolderName) && !normalizedNewFolderName.includes('/')

  const addFolder = () => {
    if (!canCreateFolder) return
    const path = selectedFolder ? `${selectedFolder}/${normalizedNewFolderName}` : normalizedNewFolderName
    onNewFolderChange(path)
    setExpanded((current) => new Set([...current, ...notebookFolderAncestors(path)]))
    setCreatingFolder(false)
    setNewFolderName('')
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-gray-950/25 px-4">
      <div className="w-full max-w-lg rounded-lg border border-blue-100 bg-white shadow-2xl shadow-blue-950/15">
        <div className="flex items-start gap-3 border-b border-gray-100 px-5 py-4">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
            <Folder size={18} />
          </div>
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-gray-950">保存到笔记本</h3>
            <p className="mt-1 text-xs text-gray-500">选择 Notebook 文件夹并确认最终文件名。</p>
          </div>
        </div>
        <div className="space-y-4 px-5 py-4">
          <div>
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-medium text-gray-600">Notebook 文件夹</span>
              <button
                className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-blue-700 hover:bg-blue-50 disabled:opacity-50"
                type="button"
                disabled={busy}
                onClick={() => setCreatingFolder((value) => !value)}
              >
                <FolderPlus size={14} />
                新建文件夹
              </button>
            </div>
            <div className="max-h-56 overflow-y-auto rounded-lg border border-gray-200 bg-gray-50/60 p-1" role="tree" aria-label="Notebook folders">
              <button
                className={`flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm ${effectiveFolder === '' ? 'bg-blue-100 text-blue-900' : 'text-gray-700 hover:bg-white'}`}
                type="button"
                disabled={busy}
                onClick={() => onSelectedFolderChange('')}
              >
                <span className="w-4" />
                <Folder size={15} />
                <span className="truncate">根目录</span>
                {effectiveFolder === '' && <CheckCircle2 className="ml-auto" size={15} />}
              </button>
              {tree.map((node) => (
                <NotebookFolderTreeRow
                  key={node.path}
                  node={node}
                  depth={0}
                  selectedFolder={effectiveFolder}
                  expanded={expanded}
                  busy={busy}
                  onSelect={(folder) => {
                    if (folder === newFolder) return
                    onSelectedFolderChange(folder)
                  }}
                  onToggle={(folder) => setExpanded((current) => {
                    const next = new Set(current)
                    if (next.has(folder)) next.delete(folder)
                    else next.add(folder)
                    return next
                  })}
                />
              ))}
              {tree.length === 0 && (
                <p className="px-8 py-3 text-xs text-gray-400">还没有文件夹，可以直接保存到根目录。</p>
              )}
            </div>
          </div>
          {creatingFolder && (
            <div className="rounded-lg border border-blue-100 bg-blue-50/50 p-3">
              <div className="mb-2 text-xs text-gray-600">
                在 <span className="font-medium text-gray-900">{selectedFolder || '根目录'}</span> 中新建
              </div>
              <div className="flex gap-2">
                <input
                  className="h-9 min-w-0 flex-1 rounded-md border border-gray-200 bg-white px-3 text-sm outline-none focus:border-blue-300 focus:ring-2 focus:ring-blue-100"
                  value={newFolderName}
                  autoFocus
                  placeholder="文件夹名称"
                  onChange={(event) => setNewFolderName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') addFolder()
                  }}
                />
                <button
                  className="h-9 rounded-md bg-blue-600 px-3 text-sm font-medium text-white disabled:bg-blue-300"
                  type="button"
                  disabled={!canCreateFolder}
                  onClick={addFolder}
                >
                  创建
                </button>
              </div>
            </div>
          )}
          <label className="block">
            <span className="text-xs font-medium text-gray-600">文件名</span>
            <input
              className="mt-1 h-10 w-full rounded-lg border border-gray-200 px-3 text-sm text-gray-800 outline-none placeholder:text-gray-400 focus:border-blue-300 focus:ring-2 focus:ring-blue-100"
              value={fileName}
              disabled={busy}
              placeholder="Conversation Note.md"
              onChange={(event) => onFileNameChange(event.target.value)}
            />
          </label>
          <div className="rounded-lg border border-gray-200 bg-gray-50 px-3 py-2">
            <div className="text-[11px] font-medium text-gray-500">保存路径</div>
            <div className="mt-1 break-all font-mono text-xs text-gray-800">{finalPath || '请输入有效的 Markdown 文件名'}</div>
          </div>
          {conflict && <p className="text-xs text-red-600">该路径已经存在，请修改文件名或选择其他文件夹。</p>}
          {error && <p className="text-xs text-red-600">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 border-t border-gray-100 px-5 py-4">
          <button
            className="inline-flex h-9 items-center rounded-lg border border-gray-200 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-60"
            type="button"
            disabled={busy}
            onClick={onCancel}
          >
            取消
          </button>
          <button
            className="inline-flex h-9 items-center gap-2 rounded-lg bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700 disabled:bg-blue-300"
            type="button"
            disabled={busy || !finalPath || conflict}
            onClick={onSave}
          >
            <FileText size={16} />
            保存
          </button>
        </div>
      </div>
    </div>
  )
}

function NotebookFolderTreeRow({
  node,
  depth,
  selectedFolder,
  expanded,
  busy,
  onSelect,
  onToggle,
}: {
  node: NotebookFolderNode
  depth: number
  selectedFolder: string
  expanded: Set<string>
  busy: boolean
  onSelect: (folder: string) => void
  onToggle: (folder: string) => void
}) {
  const hasChildren = node.children.length > 0
  const open = hasChildren && expanded.has(node.path)
  const selected = selectedFolder === node.path
  return (
    <>
      <div
        className={`flex h-8 items-center rounded-md text-sm ${selected ? 'bg-blue-100 text-blue-900' : 'text-gray-700 hover:bg-white'}`}
        style={{ paddingLeft: `${8 + depth * 18}px` }}
        role="treeitem"
        aria-selected={selected}
      >
        {hasChildren ? (
          <button
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-gray-500 hover:bg-gray-100 disabled:opacity-40"
            type="button"
            disabled={busy}
            aria-label={open ? `折叠 ${node.name}` : `展开 ${node.name}`}
            onClick={() => onToggle(node.path)}
          >
            {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        ) : <span className="h-7 w-7 shrink-0" />}
        <button
          className="flex h-full min-w-0 flex-1 items-center gap-2 text-left"
          type="button"
          disabled={busy}
          onClick={() => onSelect(node.path)}
        >
          <Folder size={15} className="shrink-0" />
          <span className="truncate">{node.name}</span>
          {selected && <CheckCircle2 className="ml-auto mr-2 shrink-0" size={15} />}
        </button>
      </div>
      {open && node.children.map((child) => (
        <NotebookFolderTreeRow
          key={child.path}
          node={child}
          depth={depth + 1}
          selectedFolder={selectedFolder}
          expanded={expanded}
          busy={busy}
          onSelect={onSelect}
          onToggle={onToggle}
        />
      ))}
    </>
  )
}

export function SaveNotebookOutcomeDialog({
  result,
  error,
  busy,
  onClose,
  onOpen,
  onRetry,
}: {
  result: SaveToNotebookResult | null
  error: string
  busy: boolean
  onClose: () => void
  onOpen: () => void
  onRetry: () => void
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-gray-950/25 px-4">
      <div className="w-full max-w-md rounded-lg border border-blue-100 bg-white shadow-2xl shadow-blue-950/15">
        <div className="px-5 py-5">
          <div className={`mb-3 flex h-10 w-10 items-center justify-center rounded-lg ${result ? 'bg-green-50 text-green-700' : error ? 'bg-red-50 text-red-700' : 'bg-blue-50 text-blue-700'}`}>
            {result ? <CheckCircle2 size={20} /> : error ? <AlertCircle size={20} /> : <Folder size={20} />}
          </div>
          <h3 className="text-sm font-semibold text-gray-950">
            {result ? '已保存到 Notebook' : error ? '无法保存到该位置' : '选择 Vault 保存位置'}
          </h3>
          <p className={`mt-2 break-all text-sm ${error ? 'text-red-600' : 'text-gray-600'}`}>
            {result?.path ?? error ?? (busy ? '正在保存...' : '请在系统保存窗口中选择 Markdown 文件。')}
          </p>
        </div>
        <div className="flex justify-end gap-2 border-t border-gray-100 px-5 py-4">
          <button className="h-9 rounded-lg border border-gray-200 px-3 text-sm font-medium text-gray-700 hover:bg-gray-50" type="button" disabled={busy} onClick={onClose}>
            关闭
          </button>
          {error && (
            <button className="h-9 rounded-lg bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" type="button" disabled={busy} onClick={onRetry}>
              重新选择
            </button>
          )}
          {result && (
            <button className="h-9 rounded-lg bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" type="button" onClick={onOpen}>
              打开笔记
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
