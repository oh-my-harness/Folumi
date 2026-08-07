import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import {
  AlertTriangle,
  BookMarked,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
  Link2,
  Network,
  NotebookPen,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Settings,
  Tags,
  Trash2,
  Undo2,
  X,
} from 'lucide-react'
import { writeClipboardText } from '../api'
import { openDesktopContextMenu } from '../desktop'
import { normalizeNotebookFileName, notebookPath } from '../notebookSave'
import type { SourceReference, SourceTarget } from './MarkdownMessage'

const NotebookLiveEditor = lazy(() => import('./NotebookLiveEditor').then((module) => ({
  default: module.NotebookLiveEditor,
})))

interface NotebookEntry {
  id: string
  space_id: string
  entry_type: string
  title: string
  path?: string | null
  markdown?: string
  metadata?: Record<string, unknown> | null
  source_session_id?: string | null
  source_message_id?: string | null
  created_at: string
  updated_at: string
  tags?: string[]
  links?: NotebookLink[]
  backlinks?: NotebookBacklink[]
  revision?: string
}

interface NotebookLink {
  raw: string
  target: string
  alias?: string | null
  target_id?: string | null
  target_title?: string | null
  resolved: boolean
}

interface NotebookBacklink {
  source_entry_id: string
  source_title: string
  raw: string
  alias?: string | null
  snippet: string
}

interface NotebookWatchInfo {
  watching?: boolean
  root?: string | null
  last_refreshed_at?: string | null
  last_result?: NotebookRefreshResult | null
  last_error?: string | null
}

interface NotebookVaultInfo {
  id: string
  name: string
  root: string
  external: boolean
  entries: number
  active: boolean
  available: boolean
}

interface NotebookRefreshResult {
  entries?: number
  folders?: number
  added?: number
  changed?: number
  unchanged?: number
  removed?: number
}

interface NotebookFolderDeleteInfo {
  path: string
  note_count: number
  file_count: number
  folder_count: number
}

interface Props {
  language: 'zh-CN' | 'en-US'
  focusTarget?: Extract<SourceTarget, { type: 'notebook' }> | null
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  onManageVaults?: () => void
}

export function NotesPage({ language, focusTarget, onManageVaults }: Props) {
  const english = language === 'en-US'
  const [entries, setEntries] = useState<NotebookEntry[]>([])
  const [folders, setFolders] = useState<string[]>([])
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(loadExpandedNotebookFolders)
  const knownFolderPathsRef = useRef<Set<string> | null>(null)
  const vaultMenuRef = useRef<HTMLDivElement | null>(null)
  const notebookRevisionsRef = useRef<Map<string, string>>(new Map())
  const [activeId, setActiveId] = useState<string | null>(null)
  const [detail, setDetail] = useState<NotebookEntry | null>(null)
  const [recentlyDeleted, setRecentlyDeleted] = useState<NotebookEntry | null>(null)
  const [watch, setWatch] = useState<NotebookWatchInfo | null>(null)
  const [vaults, setVaults] = useState<NotebookVaultInfo[]>([])
  const [vaultMenuOpen, setVaultMenuOpen] = useState(false)
  const [editorFlushToken, setEditorFlushToken] = useState(0)
  const editorFlushResolverRef = useRef<((saved: boolean) => void) | null>(null)
  const [query, setQuery] = useState('')
  const [folderDraft, setFolderDraft] = useState<{ parentPath: string; name: string } | null>(null)
  const [pendingFolderDelete, setPendingFolderDelete] = useState<NotebookFolderDeleteInfo | null>(null)
  const [recentlyDeletedFolder, setRecentlyDeletedFolder] = useState<{ token: string; path: string } | null>(null)
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('')

  const activeEntry = detail?.id === activeId
    ? detail
    : entries.find((entry) => entry.id === activeId) ?? null
  const allFolderPaths = useMemo(
    () => collectNotebookFolderPaths(buildNotebookTree(entries, folders)),
    [entries, folders],
  )
  const filteredEntries = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return entries
    return entries.filter((entry) => [entry.title, entry.path ?? '', ...(entry.tags ?? [])]
      .some((value) => value.toLocaleLowerCase().includes(normalized)))
  }, [entries, query])
  const filteredFolders = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return folders
    return folders.filter((folder) => folder.toLocaleLowerCase().includes(normalized))
  }, [folders, query])
  const notebookTree = useMemo(
    () => buildNotebookTree(filteredEntries, filteredFolders),
    [filteredEntries, filteredFolders],
  )
  const visibleExpandedFolders = useMemo(
    () => query.trim() ? new Set(collectNotebookFolderPaths(notebookTree)) : expandedFolders,
    [expandedFolders, notebookTree, query],
  )

  const loadNotebook = useCallback(async (forceVaultRefresh = false) => {
    setLoading(true)
    try {
      const response = await fetch(
        forceVaultRefresh ? '/api/notebook/refresh' : '/api/notebook/entries?space_id=default',
        forceVaultRefresh ? { method: 'POST' } : undefined,
      )
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const nextEntries = (data.entries ?? []) as NotebookEntry[]
      const nextFolders = ((data.folders ?? []) as string[]).filter(Boolean)
      notebookRevisionsRef.current.clear()
      setEntries(nextEntries)
      setFolders(nextFolders)
      setWatch((data.watch ?? null) as NotebookWatchInfo | null)
      setVaults((data.vaults ?? []) as NotebookVaultInfo[])
      // Force the active detail to reload so external Vault edits and rebuilt
      // link/backlink relations become visible immediately after refresh.
      setDetail(null)
      setActiveId((current) => current && nextEntries.some((entry) => entry.id === current)
        ? current
        : nextEntries[0]?.id ?? null)
      if (forceVaultRefresh) {
        const refresh = (data.refresh ?? {}) as NotebookRefreshResult
        setStatus(english
          ? `Notes refreshed: ${refresh.entries ?? nextEntries.length} total, ${refresh.added ?? 0} added, ${refresh.changed ?? 0} changed, ${refresh.removed ?? 0} removed`
          : `笔记已刷新：共 ${refresh.entries ?? nextEntries.length} 篇，新增 ${refresh.added ?? 0}，更新 ${refresh.changed ?? 0}，移除 ${refresh.removed ?? 0}`)
      } else {
        setStatus('')
      }
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english])

  const switchVault = useCallback(async (vaultId: string) => {
    if (loading || vaults.find((vault) => vault.id === vaultId)?.active) {
      setVaultMenuOpen(false)
      return
    }
    setLoading(true)
    setVaultMenuOpen(false)
    try {
      if (activeEntry) {
        const saved = await new Promise<boolean>((resolve) => {
          const token = Date.now()
          editorFlushResolverRef.current = resolve
          setEditorFlushToken(token)
          window.setTimeout(() => {
            if (editorFlushResolverRef.current === resolve) {
              editorFlushResolverRef.current = null
              resolve(false)
            }
          }, 3000)
        })
        if (!saved) throw new Error(english ? 'Could not save the current note. The library was not switched.' : '当前笔记保存失败，未切换笔记库。')
      }
      const response = await fetch(`/api/notebook/vaults/${encodeURIComponent(vaultId)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ active: true }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const nextEntries = (data.entries ?? []) as NotebookEntry[]
      notebookRevisionsRef.current.clear()
      knownFolderPathsRef.current = null
      setEntries(nextEntries)
      setFolders(((data.folders ?? []) as string[]).filter(Boolean))
      setVaults((data.vaults ?? []) as NotebookVaultInfo[])
      setWatch((data.watch ?? null) as NotebookWatchInfo | null)
      setDetail(null)
      setActiveId(nextEntries[0]?.id ?? null)
      setRecentlyDeleted(null)
      setRecentlyDeletedFolder(null)
      setStatus('')
      window.dispatchEvent(new Event('folumi:notebook-vaults-changed'))
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [activeEntry, loading, vaults])

  const loadDetail = useCallback(async (entryId: string) => {
    try {
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(entryId)}`)
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const entry = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      notebookRevisionsRef.current.set(entry.id, entry.revision ?? '')
      setDetail(entry)
      setEntries((items) => items.map((item) => item.id === entry.id ? { ...item, ...entry } : item))
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    }
  }, [])

  useEffect(() => { void loadNotebook() }, [loadNotebook])

  useEffect(() => {
    if (!vaultMenuOpen) return
    const close = (event: PointerEvent) => {
      if (!vaultMenuRef.current?.contains(event.target as Node)) setVaultMenuOpen(false)
    }
    document.addEventListener('pointerdown', close, true)
    return () => document.removeEventListener('pointerdown', close, true)
  }, [vaultMenuOpen])

  useEffect(() => {
    if (!activeId) {
      setDetail(null)
      return
    }
    if (detail?.id !== activeId) void loadDetail(activeId)
  }, [activeId, detail?.id, loadDetail])

  useEffect(() => {
    if (!focusTarget?.entryId) return
    setActiveId(focusTarget.entryId)
  }, [focusTarget?.entryId])

  useEffect(() => {
    if (allFolderPaths.length === 0 && !knownFolderPathsRef.current) return
    const currentPaths = new Set(allFolderPaths)
    const knownPaths = knownFolderPathsRef.current
    setExpandedFolders((current) => {
      const next = new Set<string>()
      for (const folderPath of current) {
        if (currentPaths.has(folderPath)) next.add(folderPath)
      }
      if (knownPaths) {
        for (const folderPath of currentPaths) {
          if (!knownPaths.has(folderPath)) next.add(folderPath)
        }
      }
      return setsEqual(current, next) ? current : next
    })
    knownFolderPathsRef.current = currentPaths
  }, [allFolderPaths])

  useEffect(() => {
    saveExpandedNotebookFolders(expandedFolders)
  }, [expandedFolders])

  const toggleFolder = useCallback((folderPath: string) => {
    setExpandedFolders((current) => {
      const next = new Set(current)
      if (next.has(folderPath)) next.delete(folderPath)
      else next.add(folderPath)
      return next
    })
  }, [])

  const expandFolderPath = useCallback((folderPath?: string) => {
    if (!folderPath) return
    const segments = notebookPathSegments(folderPath)
    setExpandedFolders((current) => {
      const next = new Set(current)
      for (let index = 0; index < segments.length; index += 1) {
        next.add(segments.slice(0, index + 1).join('/'))
      }
      return next
    })
  }, [])

  useEffect(() => {
    if (activeEntry?.path) expandFolderPath(parentFolder(activeEntry.path))
  }, [activeEntry?.id, activeEntry?.path, expandFolderPath])

  const createEntry = useCallback(async (folderPath?: string, linkedTitle?: string) => {
    const fallbackTitle = english ? 'Untitled note' : '未命名笔记'
    const title = linkedTitle?.trim() || fallbackTitle
    const path = folderPath ? `${folderPath.replace(/\/+$/, '')}/${title}.md` : undefined
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: 'default',
          entry_type: 'note',
          title,
          path,
          markdown: `# ${title}\n\n`,
          metadata: linkedTitle ? { created_from_unresolved_link: true } : undefined,
        }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const entry = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      notebookRevisionsRef.current.set(entry.id, entry.revision ?? '')
      expandFolderPath(folderPath)
      setEntries((items) => [entry, ...items.filter((item) => item.id !== entry.id)])
      setActiveId(entry.id)
      setDetail(entry)
      setQuery('')
      setStatus(linkedTitle
        ? (english ? `Created linked note: ${entry.title}` : `已创建关联笔记：${entry.title}`)
        : (english ? 'Note created' : '笔记已创建'))
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english, expandFolderPath])

  const startCreateFolder = useCallback((parentPath?: string) => {
    const normalizedParent = parentPath?.replace(/\/+$/, '') ?? ''
    setQuery('')
    expandFolderPath(normalizedParent)
    setFolderDraft({ parentPath: normalizedParent, name: '' })
  }, [expandFolderPath])

  const createFolder = useCallback(async (parentPath: string, name: string) => {
    const trimmedName = name.trim()
    if (!trimmedName || loading) {
      setFolderDraft(null)
      return
    }
    const path = parentPath ? `${parentPath}/${trimmedName}` : trimmedName
    setFolderDraft(null)
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/folders', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const nextFolders = ((data.folders ?? []) as string[]).filter(Boolean)
      const createdPath = ((data.folder as { path?: string } | undefined)?.path ?? path)
      setFolders(nextFolders)
      expandFolderPath(createdPath)
      setQuery('')
      setStatus(english ? `Created folder: ${createdPath}` : `已创建文件夹：${createdPath}`)
    } catch (error) {
      setFolderDraft({ parentPath, name: trimmedName })
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english, expandFolderPath, loading])

  const renameEntry = useCallback(async (entry: NotebookEntry, name: string) => {
    const fileName = normalizeNotebookFileName(name)
    if (!fileName) {
      setStatus(english ? 'Enter a file name' : '请输入文件名')
      return false
    }
    const path = notebookPath(parentFolder(entry.path) ?? '', fileName)
    if (!path || path === entry.path) return true
    setLoading(true)
    try {
      let revision = entry.revision
      if (!revision) {
        const detailResponse = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`)
        const detailData = await safeJson(detailResponse)
        if (!detailResponse.ok) throw new Error(errorMessage(detailData, detailResponse.status))
        revision = detailData.revision as string
      }
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expected_revision: revision, path }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const updated = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      notebookRevisionsRef.current.set(updated.id, updated.revision ?? '')
      setEntries((items) => items.map((item) => item.id === updated.id ? { ...item, ...updated } : item))
      setDetail((current) => current?.id === updated.id ? { ...current, ...updated } : current)
      setStatus(english ? `Renamed note to ${fileName}` : `笔记已重命名为 ${fileName}`)
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
      return false
    } finally {
      setLoading(false)
    }
  }, [english])

  const renameFolder = useCallback(async (folderPath: string, name: string) => {
    const trimmedName = name.trim()
    if (!trimmedName || /[\\/]/.test(trimmedName)) {
      setStatus(english ? 'Enter one folder name without slashes' : '请输入不含斜杠的目录名')
      return false
    }
    const parentSegments = notebookPathSegments(folderPath)
    parentSegments.pop()
    const parentPath = parentSegments.join('/')
    const newPath = parentPath ? `${parentPath}/${trimmedName}` : trimmedName
    if (newPath === folderPath) return true
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/folders', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: folderPath, new_path: newPath }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const renamedPath = ((data.folder as { path?: string } | undefined)?.path ?? newPath)
      const nextEntries = (data.entries ?? []) as NotebookEntry[]
      notebookRevisionsRef.current.clear()
      setEntries(nextEntries)
      setFolders(((data.folders ?? []) as string[]).filter(Boolean))
      setDetail((current) => {
        if (!current) return current
        const updated = nextEntries.find((entry) => entry.id === current.id)
        return updated ? { ...current, ...updated } : current
      })
      setExpandedFolders((current) => new Set([...current].map((path) => replaceNotebookFolderPrefix(path, folderPath, renamedPath))))
      setStatus(english ? `Renamed folder to ${renamedPath}` : `目录已重命名为 ${renamedPath}`)
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
      return false
    } finally {
      setLoading(false)
    }
  }, [english])

  const deleteFolder = useCallback(async (folderPath: string) => {
    setLoading(true)
    try {
      const response = await fetch(`/api/notebook/folders?path=${encodeURIComponent(folderPath)}`, {
        method: 'DELETE',
      })
      const data = await safeJson(response)
      if (!response.ok) {
        if (response.status === 409 && data.code === 'folder_not_empty' && data.folder) {
          setPendingFolderDelete(data.folder as unknown as NotebookFolderDeleteInfo)
          return
        }
        throw new Error(errorMessage(data, response.status))
      }
      setFolders(((data.folders ?? []) as string[]).filter(Boolean))
      setExpandedFolders((current) => {
        const next = new Set(current)
        next.delete(folderPath)
        return next
      })
      setStatus(english ? `Deleted folder: ${folderPath}` : `已删除目录：${folderPath}`)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english])

  const confirmRecursiveFolderDelete = useCallback(async () => {
    if (!pendingFolderDelete) return
    const folderPath = pendingFolderDelete.path
    setLoading(true)
    try {
      const response = await fetch(`/api/notebook/folders?path=${encodeURIComponent(folderPath)}&recursive=true`, {
        method: 'DELETE',
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const nextEntries = (data.entries ?? []) as NotebookEntry[]
      notebookRevisionsRef.current.clear()
      setEntries(nextEntries)
      setFolders(((data.folders ?? []) as string[]).filter(Boolean))
      setActiveId((current) => current && nextEntries.some((entry) => entry.id === current) ? current : null)
      setDetail(null)
      setExpandedFolders((current) => {
        const next = new Set<string>()
        for (const path of current) {
          if (path !== folderPath && !path.startsWith(`${folderPath}/`)) next.add(path)
        }
        return next
      })
      const deleted = data.deleted as { token?: string; folder?: NotebookFolderDeleteInfo } | undefined
      if (deleted?.token) setRecentlyDeletedFolder({ token: deleted.token, path: folderPath })
      setPendingFolderDelete(null)
      setStatus(english ? `Deleted folder: ${folderPath}` : `已删除目录：${folderPath}`)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english, pendingFolderDelete])

  const restoreRecentlyDeletedFolder = useCallback(async () => {
    if (!recentlyDeletedFolder) return
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/folders/restore', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token: recentlyDeletedFolder.token }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      notebookRevisionsRef.current.clear()
      setEntries((data.entries ?? []) as NotebookEntry[])
      setFolders(((data.folders ?? []) as string[]).filter(Boolean))
      expandFolderPath(recentlyDeletedFolder.path)
      setRecentlyDeletedFolder(null)
      setStatus(english ? 'Folder restored' : '目录已恢复')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english, expandFolderPath, recentlyDeletedFolder])

  const saveEntry = useCallback(async (entryId: string, markdown: string) => {
    try {
      let revision = notebookRevisionsRef.current.get(entryId)
      if (!revision) {
        const detailResponse = await fetch(`/api/notebook/entries/${encodeURIComponent(entryId)}`)
        const detailData = await safeJson(detailResponse)
        if (!detailResponse.ok) throw new Error(errorMessage(detailData, detailResponse.status))
        revision = detailData.revision as string
      }
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(entryId)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_revision: revision,
          markdown,
        }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const updated = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      notebookRevisionsRef.current.set(updated.id, updated.revision ?? '')
      setEntries((items) => items.map((item) => item.id === updated.id ? { ...item, ...updated } : item))
      setDetail((current) => current?.id === updated.id ? { ...current, ...updated } : current)
      setStatus('')
      await loadDetail(updated.id)
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
      return false
    }
  }, [loadDetail])

  const deleteEntry = useCallback(async (entry: NotebookEntry) => {
    if (!window.confirm(english ? `Delete “${entry.title}”?` : `确定删除“${entry.title}”吗？`)) return
    const previousEntries = entries
    try {
      let restorable = detail?.id === entry.id ? detail : entry
      if (restorable.markdown === undefined) {
        const detailResponse = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`)
        const detailData = await safeJson(detailResponse)
        if (!detailResponse.ok) throw new Error(errorMessage(detailData, detailResponse.status))
        restorable = { ...(detailData.entry as NotebookEntry), revision: detailData.revision as string }
      }
      setEntries((items) => items.filter((item) => item.id !== entry.id))
      setActiveId((current) => current === entry.id ? null : current)
      notebookRevisionsRef.current.delete(entry.id)
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`, { method: 'DELETE' })
      if (!response.ok) throw new Error(errorMessage(await safeJson(response), response.status))
      setRecentlyDeleted(restorable)
      setStatus(english ? 'Note deleted' : '笔记已删除')
    } catch (error) {
      setEntries(previousEntries)
      setStatus(error instanceof Error ? error.message : String(error))
    }
  }, [detail, english, entries])

  const restoreRecentlyDeleted = useCallback(async () => {
    if (!recentlyDeleted) return
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: recentlyDeleted.space_id,
          entry_type: recentlyDeleted.entry_type,
          title: recentlyDeleted.title,
          path: recentlyDeleted.path,
          markdown: recentlyDeleted.markdown,
          metadata: recentlyDeleted.metadata,
          source_session_id: recentlyDeleted.source_session_id,
          source_message_id: recentlyDeleted.source_message_id,
        }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const restored = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      setEntries((items) => [restored, ...items])
      setActiveId(restored.id)
      setDetail(restored)
      expandFolderPath(parentFolder(restored.path))
      setRecentlyDeleted(null)
      setStatus(english ? 'Note restored' : '笔记已恢复')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [english, expandFolderPath, recentlyDeleted])

  const openRootContextMenu = useCallback((event: MouseEvent) => {
    if (vaults.length === 0) return
    const target = event.target
    if (target instanceof Element && target.closest('[data-notebook-tree-row="true"]')) return
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      { label: english ? 'New Note' : '新建笔记', run: () => { void createEntry() } },
      { label: english ? 'New Folder' : '新建目录', run: () => startCreateFolder() },
      ...(watch?.root ? [{
        label: english ? 'Copy folder path' : '复制文件夹路径',
        run: () => { void writeClipboardText(watch.root ?? '') },
      }] : []),
    ])
    if (opened) event.preventDefault()
  }, [createEntry, english, startCreateFolder, vaults.length, watch?.root])

  return (
    <main className="flex h-full min-h-0 flex-col bg-white">
      <header className="flex items-center gap-4 border-b border-gray-200 px-6 py-3">
        <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
          <BookMarked size={21} />
        </span>
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-semibold text-gray-950">{english ? 'Notebook' : '笔记'}</h1>
          <p className="mt-0.5 truncate text-sm text-gray-500">
            {english ? 'Write, organize, and connect your ideas.' : '写下、整理并连接你的想法。'}
          </p>
        </div>
        <div ref={vaultMenuRef} className="relative">
          <button
            type="button"
            className="group inline-flex h-9 max-w-64 items-center gap-2 rounded-full border border-sky-100 bg-gradient-to-r from-sky-50 to-indigo-50/70 px-2.5 pr-3 text-sm font-medium text-slate-700 transition hover:border-sky-200 hover:from-sky-100 hover:to-indigo-50 disabled:opacity-50"
            disabled={loading}
            aria-expanded={vaultMenuOpen}
            onClick={() => {
              if (vaults.length === 0 && onManageVaults) {
                onManageVaults()
                return
              }
              setVaultMenuOpen((open) => !open)
            }}
          >
            <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-white text-sky-600 shadow-sm shadow-sky-100">
              {vaults.length === 0 ? <Plus size={14} /> : <FolderOpen size={14} />}
            </span>
            <span className="truncate">{vaults.find((vault) => vault.active)?.name ?? (english ? 'Add note library' : '添加笔记库')}</span>
            {vaults.length > 0 && <ChevronDown size={14} className="shrink-0 text-slate-400 transition group-hover:text-sky-600" />}
          </button>
          {vaultMenuOpen && (
            <div className="absolute right-0 top-[calc(100%+0.55rem)] z-50 w-64 overflow-hidden rounded-2xl border border-slate-100 bg-white/95 p-2 shadow-[0_16px_45px_-18px_rgba(30,64,175,0.3)] backdrop-blur">
              {vaults.map((vault) => (
                <button
                  key={vault.id}
                  type="button"
                  disabled={!vault.available}
                  className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm transition disabled:cursor-not-allowed disabled:opacity-50 ${vault.active ? 'bg-sky-50 text-sky-800' : 'text-slate-700 hover:bg-slate-50'}`}
                  onClick={() => void switchVault(vault.id)}
                  title={vault.root}
                >
                  <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-xl ${vault.active ? 'bg-white text-sky-600 shadow-sm' : 'bg-slate-50 text-slate-400'}`}><Folder size={15} /></span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{vault.name}</span>
                    <span className="mt-0.5 block truncate text-xs text-slate-400">{vault.available ? `${vault.entries} ${english ? 'notes' : '篇笔记'}` : (english ? 'Unavailable' : '无法访问')}</span>
                  </span>
                  {vault.active && <span className="h-2 w-2 rounded-full bg-sky-500 ring-4 ring-sky-100" />}
                </button>
              ))}
              {onManageVaults && (
                <button
                  type="button"
                  className="mt-1 flex w-full items-center gap-3 border-t border-slate-100 px-3 py-2.5 text-left text-sm text-slate-500 transition hover:bg-slate-50 hover:text-sky-700"
                  onClick={() => {
                    setVaultMenuOpen(false)
                    onManageVaults()
                  }}
                >
                  <Settings size={16} />
                  {english ? 'Manage note libraries' : '管理笔记库'}
                </button>
              )}
            </div>
          )}
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-80 shrink-0 flex-col border-r border-gray-200 bg-gray-50/70">
          <div className="border-b border-gray-200 px-3 py-3">
            <div className="mb-2 flex items-center justify-between px-1">
              <span className="text-[11px] font-semibold uppercase tracking-wider text-gray-500">
                {english ? 'Explorer' : '资源管理器'}
              </span>
              <span className="text-[11px] tabular-nums text-gray-400">
                {entries.length} {english ? 'notes' : '篇'}
              </span>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <button className={compactButtonClassName} type="button" disabled={loading || vaults.length === 0} onClick={() => void createEntry()}>
                <Plus size={14} />{english ? 'New note' : '新建笔记'}
              </button>
              <button className={compactButtonClassName} type="button" disabled={loading || vaults.length === 0} onClick={() => startCreateFolder()}>
                <Folder size={14} />{english ? 'New folder' : '新建目录'}
              </button>
            </div>
            <input
              className={`${inputClassName} mt-2 h-8 py-1.5 text-xs`}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={english ? 'Filter title, path, or tag' : '筛选标题、路径或标签'}
              aria-label={english ? 'Filter notes' : '筛选笔记'}
            />
          </div>

          {recentlyDeleted && (
            <div className="flex items-center gap-2 border-b border-amber-100 bg-amber-50 px-3 py-2 text-xs text-amber-900">
              <span className="min-w-0 flex-1 truncate">
                {english ? `Deleted “${recentlyDeleted.title}”` : `已删除“${recentlyDeleted.title}”`}
              </span>
              <button type="button" className="inline-flex items-center gap-1 font-medium" onClick={() => void restoreRecentlyDeleted()}>
                <Undo2 size={13} />{english ? 'Undo' : '撤销'}
              </button>
              <button type="button" aria-label={english ? 'Dismiss' : '关闭'} onClick={() => setRecentlyDeleted(null)}><X size={13} /></button>
            </div>
          )}

          {recentlyDeletedFolder && (
            <div className="flex items-center gap-2 border-b border-amber-100 bg-amber-50 px-3 py-2 text-xs text-amber-900">
              <span className="min-w-0 flex-1 truncate">
                {english ? `Deleted folder “${recentlyDeletedFolder.path}”` : `已删除目录“${recentlyDeletedFolder.path}”`}
              </span>
              <button type="button" className="inline-flex items-center gap-1 font-medium" disabled={loading} onClick={() => void restoreRecentlyDeletedFolder()}>
                <Undo2 size={13} />{english ? 'Undo' : '撤销'}
              </button>
              <button type="button" aria-label={english ? 'Dismiss' : '关闭'} onClick={() => setRecentlyDeletedFolder(null)}><X size={13} /></button>
            </div>
          )}

          <div
            className="min-h-0 flex-1 px-2 py-2"
            data-surface-context-menu="true"
            onContextMenu={openRootContextMenu}
          >
            {notebookTree.length === 0 && !folderDraft ? (
              <div className="px-3 py-10 text-center text-sm text-gray-400">
                {query
                  ? (english ? 'No matching notes' : '没有匹配的笔记')
                  : (english ? 'No notes yet' : '还没有笔记')}
              </div>
            ) : (
              <NotebookFileTree
                nodes={notebookTree}
                activeEntryId={activeId}
                expandedFolders={visibleExpandedFolders}
                folderDraft={folderDraft}
                language={language}
                onToggleFolder={toggleFolder}
                onSelectEntry={setActiveId}
                onCreateEntry={(folderPath) => void createEntry(folderPath)}
                onCreateFolder={startCreateFolder}
                onFolderDraftNameChange={(name) => setFolderDraft((current) => current ? { ...current, name } : null)}
                onCommitFolder={(parentPath, name) => void createFolder(parentPath, name)}
                onCancelFolder={() => setFolderDraft(null)}
                onRenameFolder={renameFolder}
                onRenameEntry={renameEntry}
                onDeleteFolder={(folderPath) => void deleteFolder(folderPath)}
                onDeleteEntry={(entry) => void deleteEntry(entry)}
              />
            )}
          </div>
          {(status || watch?.last_error) && (
            <div className={`border-t px-3 py-2 text-xs ${watch?.last_error ? 'border-red-100 bg-red-50 text-red-700' : 'border-gray-200 text-gray-500'}`}>
              {watch?.last_error || status}
            </div>
          )}
        </aside>

        <NotebookEditor
          entry={activeEntry}
          language={language}
          onSave={saveEntry}
          onCreateEntry={() => void createEntry()}
          onCreateLinkedEntry={(title) => void createEntry(undefined, title)}
          onSelectEntry={setActiveId}
          hasVault={vaults.length > 0}
          onManageVaults={onManageVaults}
          flushToken={editorFlushToken}
          onFlushComplete={(token, saved) => {
            if (token !== editorFlushToken) return
            const resolve = editorFlushResolverRef.current
            editorFlushResolverRef.current = null
            resolve?.(saved)
          }}
        />
      </div>
      {pendingFolderDelete && (
        <FolderDeleteDialog
          folder={pendingFolderDelete}
          language={language}
          loading={loading}
          onCancel={() => setPendingFolderDelete(null)}
          onConfirm={() => void confirmRecursiveFolderDelete()}
        />
      )}
    </main>
  )
}

function FolderDeleteDialog({ folder, language, loading, onCancel, onConfirm }: {
  folder: NotebookFolderDeleteInfo
  language: 'zh-CN' | 'en-US'
  loading: boolean
  onCancel: () => void
  onConfirm: () => void
}) {
  const english = language === 'en-US'
  const otherFiles = Math.max(0, folder.file_count - folder.note_count)
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !loading) onCancel()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [loading, onCancel])
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-gray-950/25 px-4"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !loading) onCancel()
      }}
    >
      <section className="w-full max-w-md rounded-2xl border border-gray-200 bg-white p-5 shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="notebook-folder-delete-title">
        <div className="flex items-start gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-red-50 text-red-600"><AlertTriangle size={20} /></span>
          <div className="min-w-0">
            <h2 id="notebook-folder-delete-title" className="text-base font-semibold text-gray-950">{english ? 'This folder is not empty' : '目录不是空目录'}</h2>
            <p className="mt-1 break-all text-sm text-gray-500">{folder.path}</p>
          </div>
        </div>
        <p className="mt-4 text-sm leading-6 text-gray-700">
          {english
            ? `Continuing will remove ${folder.note_count} notes, ${otherFiles} other files, and ${folder.folder_count} subfolders. You can undo this deletion afterward.`
            : `继续删除将移除 ${folder.note_count} 篇笔记、${otherFiles} 个其他文件和 ${folder.folder_count} 个子目录。删除后仍可撤销恢复。`}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <button autoFocus className={compactButtonClassName} type="button" disabled={loading} onClick={onCancel}>{english ? 'Cancel' : '取消'}</button>
          <button className="inline-flex h-9 items-center justify-center rounded-lg bg-red-600 px-4 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50" type="button" disabled={loading} onClick={onConfirm}>
            {loading ? (english ? 'Deleting…' : '正在删除…') : (english ? 'Delete anyway' : '仍要删除')}
          </button>
        </div>
      </section>
    </div>
  )
}

function NotebookEditor({
  entry,
  language,
  onSave,
  onCreateEntry,
  onCreateLinkedEntry,
  onSelectEntry,
  hasVault,
  onManageVaults,
  flushToken,
  onFlushComplete,
}: {
  entry: NotebookEntry | null
  language: 'zh-CN' | 'en-US'
  onSave: (entryId: string, markdown: string) => Promise<boolean>
  onCreateEntry: () => void
  onCreateLinkedEntry: (title: string) => void
  onSelectEntry: (id: string) => void
  hasVault: boolean
  onManageVaults?: () => void
  flushToken: number
  onFlushComplete: (token: number, saved: boolean) => void
}) {
  const english = language === 'en-US'
  const [relationsCollapsed, setRelationsCollapsed] = useState(true)

  const resolveWikiLink = useCallback((target: string) => {
    if (!entry) return undefined
    const normalized = target.trim().toLocaleLowerCase()
    return (entry.links ?? []).find((item) => item.target.trim().toLocaleLowerCase() === normalized
      || item.target_title?.trim().toLocaleLowerCase() === normalized
      || item.target_id === target)
  }, [entry])

  if (!entry) {
    if (!hasVault) {
      return (
        <section className="flex min-w-0 flex-1 items-center justify-center bg-gradient-to-b from-sky-50/30 to-white px-6">
          <div className="max-w-sm text-center">
            <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-sky-50 text-sky-600"><FolderOpen size={26} /></div>
            <h2 className="mt-5 text-xl font-semibold text-slate-900">{english ? 'Add a note library' : '添加一个笔记库'}</h2>
            <p className="mt-2 text-sm leading-6 text-slate-500">{english ? 'Choose a folder to start writing and organizing notes.' : '选择一个文件夹，就可以开始记录和整理笔记。'}</p>
            {onManageVaults && (
              <button className={`${primaryCompactButtonClassName} mx-auto mt-5 px-4`} type="button" onClick={onManageVaults}>
                <Plus size={15} />{english ? 'Add note library' : '添加笔记库'}
              </button>
            )}
          </div>
        </section>
      )
    }
    return (
      <section className="flex min-w-0 flex-1 items-center justify-center px-6">
        <div className="max-w-md text-center">
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50 text-blue-700"><BookMarked size={28} /></div>
          <h2 className="mt-5 text-xl font-semibold text-gray-950">{english ? 'Create or select a note' : '创建或选择一份笔记'}</h2>
          <p className="mt-2 text-sm leading-6 text-gray-500">
            {english ? 'Use folders, Wiki links, tags, and backlinks to build your personal knowledge network.' : '使用目录、Wiki 链接、标签和反向链接构建你的个人知识网络。'}
          </p>
          <button className={`${primaryCompactButtonClassName} mx-auto mt-5 px-4`} type="button" onClick={onCreateEntry}>
            <Plus size={15} />{english ? 'New note' : '新建笔记'}
          </button>
        </div>
      </section>
    )
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
          <Suspense fallback={<div className="flex min-h-full items-center justify-center text-sm text-gray-400">{english ? 'Loading editor…' : '正在加载编辑器…'}</div>}>
            <NotebookLiveEditor
              key={entry.id}
              entryId={entry.id}
              markdown={entry.markdown || ''}
              language={language}
              onSave={onSave}
              flushToken={flushToken}
              onFlushComplete={onFlushComplete}
              onWikiLinkOpen={(target) => {
                const link = resolveWikiLink(target)
                if (link?.target_id) onSelectEntry(link.target_id)
                else onCreateLinkedEntry(target)
              }}
            />
          </Suspense>
        </div>
        <NotebookRelationsPanel
          entry={entry}
          collapsed={relationsCollapsed}
          language={language}
          onCollapsedChange={setRelationsCollapsed}
          onSelectEntry={onSelectEntry}
          onCreateLinkedEntry={onCreateLinkedEntry}
        />
      </div>
    </section>
  )
}

function NotebookRelationsPanel({ entry, collapsed, language, onCollapsedChange, onSelectEntry, onCreateLinkedEntry }: {
  entry: NotebookEntry
  collapsed: boolean
  language: 'zh-CN' | 'en-US'
  onCollapsedChange: (collapsed: boolean) => void
  onSelectEntry: (id: string) => void
  onCreateLinkedEntry: (title: string) => void
}) {
  const english = language === 'en-US'
  const tags = entry.tags ?? []
  const links = entry.links ?? []
  const backlinks = entry.backlinks ?? []
  const unresolvedLinks = links.filter((link) => !link.resolved)
  const [localGraphCollapsed, setLocalGraphCollapsed] = useState(false)

  if (collapsed) {
    return (
      <aside className="hidden w-12 shrink-0 border-l border-gray-200 bg-white px-1.5 py-4 xl:flex xl:flex-col xl:items-center">
        <button className={iconButtonClassName} type="button" title={english ? 'Expand relations' : '展开关联信息'} aria-label={english ? 'Expand relations' : '展开关联信息'} onClick={() => onCollapsedChange(false)}><PanelRightOpen size={18} /></button>
        <div className="mt-5 rotate-90 whitespace-nowrap text-[11px] font-semibold uppercase tracking-wider text-gray-400">{english ? 'Relations' : '关联'}</div>
      </aside>
    )
  }

  return (
    <aside className="hidden w-80 shrink-0 overflow-y-auto border-l border-gray-200 bg-white px-4 py-4 xl:block">
      <div className="space-y-6">
        <div className="flex items-center justify-between gap-3">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-gray-500">{english ? 'Relations' : '关联信息'}</div>
          <button className={iconButtonClassName} type="button" title={english ? 'Collapse relations' : '收起关联信息'} aria-label={english ? 'Collapse relations' : '收起关联信息'} onClick={() => onCollapsedChange(true)}><PanelRightClose size={16} /></button>
        </div>

        <NotebookLocalGraph entry={entry} links={links} backlinks={backlinks} collapsed={localGraphCollapsed} language={language} onCollapsedChange={setLocalGraphCollapsed} onSelectEntry={onSelectEntry} onCreateLinkedEntry={onCreateLinkedEntry} />

        <RelationSection icon={AlertTriangle} title={english ? 'Unresolved links' : '未解析链接'}>
          {unresolvedLinks.length === 0 ? <EmptyRelation text={english ? 'All outgoing links resolve' : '所有出链均已解析'} /> : (
            <div className="space-y-2">{unresolvedLinks.map((link) => (
              <button key={`unresolved:${link.raw}:${link.target}`} className="w-full rounded-lg border border-dashed border-amber-200 bg-amber-50 px-3 py-2 text-left text-sm text-amber-800 hover:bg-amber-100" type="button" onClick={() => onCreateLinkedEntry(link.target)}>
                <span className="block truncate font-medium">{link.alias || link.target}</span>
                <span className="mt-0.5 block truncate text-xs text-amber-700">{english ? 'Create note' : '点击创建笔记'} · {link.raw}</span>
              </button>
            ))}</div>
          )}
        </RelationSection>

        <RelationSection icon={Tags} title={english ? 'Tags' : '标签'}>
          {tags.length === 0 ? <EmptyRelation text={english ? 'No tags yet' : '暂无标签'} /> : (
            <div className="flex flex-wrap gap-2">{tags.map((tag) => <span key={tag} className="rounded-full bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700">#{tag}</span>)}</div>
          )}
        </RelationSection>

        <RelationSection icon={Link2} title={english ? 'Outgoing links' : '出链'}>
          {links.length === 0 ? <EmptyRelation text={english ? 'No outgoing links' : '暂无出链'} /> : (
            <div className="space-y-2">{links.map((link) => (
              <button key={`${link.raw}:${link.target_id ?? link.target}`} className={`w-full rounded-lg border px-3 py-2 text-left text-sm ${link.resolved ? 'border-blue-100 bg-blue-50/60 text-blue-800 hover:bg-blue-100' : 'border-dashed border-amber-200 bg-amber-50/70 text-amber-700 hover:bg-amber-100'}`} type="button" onClick={() => link.target_id ? onSelectEntry(link.target_id) : onCreateLinkedEntry(link.target)}>
                <span className="block truncate font-medium">{link.alias || link.target_title || link.target}</span>
                <span className="mt-0.5 block text-xs opacity-75">{link.resolved ? (english ? 'Resolved note' : '已解析笔记') : (english ? 'Create note' : '创建笔记')}</span>
              </button>
            ))}</div>
          )}
        </RelationSection>

        <RelationSection icon={NotebookPen} title={english ? 'Backlinks' : '反向链接'}>
          {backlinks.length === 0 ? <EmptyRelation text={english ? 'No backlinks yet' : '暂无反向链接'} /> : (
            <div className="space-y-2">{backlinks.map((backlink) => (
              <button key={`${backlink.source_entry_id}:${backlink.raw}`} className="w-full rounded-lg border border-gray-100 bg-gray-50 px-3 py-2 text-left text-sm text-gray-700 hover:border-blue-100 hover:bg-blue-50" type="button" onClick={() => onSelectEntry(backlink.source_entry_id)}>
                <span className="block truncate font-medium text-gray-900">{backlink.source_title}</span>
                <span className="mt-1 line-clamp-2 text-xs leading-5 text-gray-500">{backlink.snippet}</span>
              </button>
            ))}</div>
          )}
        </RelationSection>
      </div>
    </aside>
  )
}

function RelationSection({ icon: Icon, title, children }: { icon: typeof Tags; title: string; children: React.ReactNode }) {
  return <section><div className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-gray-500"><Icon size={14} />{title}</div>{children}</section>
}

function EmptyRelation({ text }: { text: string }) {
  return <div className="text-sm text-gray-400">{text}</div>
}

function NotebookLocalGraph({ entry, links, backlinks, collapsed, language, onCollapsedChange, onSelectEntry, onCreateLinkedEntry }: {
  entry: NotebookEntry
  links: NotebookLink[]
  backlinks: NotebookBacklink[]
  collapsed: boolean
  language: 'zh-CN' | 'en-US'
  onCollapsedChange: (collapsed: boolean) => void
  onSelectEntry: (id: string) => void
  onCreateLinkedEntry: (title: string) => void
}) {
  const english = language === 'en-US'
  const outgoing = links.slice(0, 5)
  const incoming = backlinks.slice(0, 5)
  const hasNodes = outgoing.length > 0 || incoming.length > 0
  return (
    <section>
      <button className="mb-2 flex w-full items-center gap-2 rounded-lg px-1 py-1 text-left text-[11px] font-semibold uppercase tracking-wider text-gray-500 hover:bg-gray-50 hover:text-blue-700" type="button" aria-expanded={!collapsed} onClick={() => onCollapsedChange(!collapsed)}>
        <Network size={14} /><span className="flex-1">{english ? 'Local graph' : '局部关系图'}</span><ChevronRight size={14} className={collapsed ? '' : 'rotate-90'} />
      </button>
      {!collapsed && <div className="rounded-xl border border-blue-50 bg-gradient-to-b from-blue-50/70 to-white p-3">
        <div className="flex justify-center"><div className="max-w-full truncate rounded-full bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white shadow-sm">{entry.title}</div></div>
        {!hasNodes ? <div className="mt-3 text-center text-xs text-gray-400">{english ? 'No linked notes yet' : '暂无关联笔记'}</div> : (
          <div className="mt-4 grid grid-cols-2 gap-2">
            <GraphColumn label={english ? 'Out' : '出链'} empty={english ? 'None' : '无'}>
              {outgoing.map((link) => <button key={`graph-out:${link.raw}:${link.target_id ?? link.target}`} className={`w-full truncate rounded-lg border px-2 py-1.5 text-left text-xs ${link.resolved ? 'border-blue-100 bg-white text-blue-800 hover:bg-blue-50' : 'border-dashed border-amber-200 bg-amber-50 text-amber-800 hover:bg-amber-100'}`} type="button" onClick={() => link.target_id ? onSelectEntry(link.target_id) : onCreateLinkedEntry(link.target)}>{link.alias || link.target_title || link.target}</button>)}
            </GraphColumn>
            <GraphColumn label={english ? 'In' : '反链'} empty={english ? 'None' : '无'}>
              {incoming.map((backlink) => <button key={`graph-in:${backlink.source_entry_id}:${backlink.raw}`} className="w-full truncate rounded-lg border border-gray-100 bg-white px-2 py-1.5 text-left text-xs text-gray-700 hover:border-blue-100 hover:bg-blue-50" type="button" onClick={() => onSelectEntry(backlink.source_entry_id)}>{backlink.source_title}</button>)}
            </GraphColumn>
          </div>
        )}
      </div>}
    </section>
  )
}

function GraphColumn({ label, empty, children }: { label: string; empty: string; children: React.ReactNode }) {
  const items = Array.isArray(children) ? children : [children]
  return <div className="space-y-2"><div className="text-[11px] font-semibold uppercase tracking-wide text-blue-700">{label}</div>{items.length === 0 ? <div className="text-xs text-gray-400">{empty}</div> : children}</div>
}

type NotebookTreeNode =
  | { type: 'folder'; name: string; path: string; children: NotebookTreeNode[] }
  | { type: 'entry'; name: string; path: string; entry: NotebookEntry }

type FlatNotebookTreeRow = { node: NotebookTreeNode; depth: number }
type NotebookTreeDisplayRow =
  | { type: 'node'; node: NotebookTreeNode; depth: number }
  | { type: 'folder_draft'; depth: number }
type NotebookTreeRenameDraft =
  | { type: 'folder'; path: string; name: string }
  | { type: 'entry'; entry: NotebookEntry; name: string }

const NOTEBOOK_TREE_ROW_HEIGHT = 32
const NOTEBOOK_TREE_OVERSCAN_ROWS = 12
const NOTEBOOK_EXPANDED_FOLDERS_KEY = 'folumi:notebook-expanded-folders:v3'

function NotebookFileTree({ nodes, activeEntryId, expandedFolders, folderDraft, language, onToggleFolder, onSelectEntry, onCreateEntry, onCreateFolder, onFolderDraftNameChange, onCommitFolder, onCancelFolder, onRenameFolder, onRenameEntry, onDeleteFolder, onDeleteEntry }: {
  nodes: NotebookTreeNode[]
  activeEntryId: string | null
  expandedFolders: Set<string>
  folderDraft: { parentPath: string; name: string } | null
  language: 'zh-CN' | 'en-US'
  onToggleFolder: (folderPath: string) => void
  onSelectEntry: (id: string) => void
  onCreateEntry: (folderPath?: string) => void
  onCreateFolder: (parentPath?: string) => void
  onFolderDraftNameChange: (name: string) => void
  onCommitFolder: (parentPath: string, name: string) => void
  onCancelFolder: () => void
  onRenameFolder: (folderPath: string, name: string) => Promise<boolean>
  onRenameEntry: (entry: NotebookEntry, name: string) => Promise<boolean>
  onDeleteFolder: (folderPath: string) => void
  onDeleteEntry: (entry: NotebookEntry) => void
}) {
  const english = language === 'en-US'
  const containerRef = useRef<HTMLDivElement | null>(null)
  const [scrollTop, setScrollTop] = useState(0)
  const [viewportHeight, setViewportHeight] = useState(360)
  const [renameDraft, setRenameDraft] = useState<NotebookTreeRenameDraft | null>(null)
  const committingRenameRef = useRef(false)
  const renameInputRef = useRef<HTMLInputElement | null>(null)
  const visibleRows = useMemo<NotebookTreeDisplayRow[]>(() => {
    const rows: NotebookTreeDisplayRow[] = flattenVisibleNotebookTree(nodes, expandedFolders)
      .map((row) => ({ type: 'node', ...row }))
    if (!folderDraft) return rows
    if (!folderDraft.parentPath) {
      rows.unshift({ type: 'folder_draft', depth: 0 })
      return rows
    }
    const parentIndex = rows.findIndex((row) => row.type === 'node'
      && row.node.type === 'folder'
      && row.node.path === folderDraft.parentPath)
    const parentDepth = parentIndex >= 0
      ? (rows[parentIndex]?.depth ?? 0)
      : notebookPathSegments(folderDraft.parentPath).length - 1
    rows.splice(parentIndex >= 0 ? parentIndex + 1 : 0, 0, { type: 'folder_draft', depth: parentDepth + 1 })
    return rows
  }, [expandedFolders, folderDraft, nodes])
  const totalHeight = visibleRows.length * NOTEBOOK_TREE_ROW_HEIGHT
  const startIndex = Math.max(0, Math.floor(scrollTop / NOTEBOOK_TREE_ROW_HEIGHT) - NOTEBOOK_TREE_OVERSCAN_ROWS)
  const endIndex = Math.min(visibleRows.length, Math.ceil((scrollTop + viewportHeight) / NOTEBOOK_TREE_ROW_HEIGHT) + NOTEBOOK_TREE_OVERSCAN_ROWS)
  const renderedRows = visibleRows.slice(startIndex, endIndex)

  useEffect(() => {
    const element = containerRef.current
    if (!element) return
    const measure = () => setViewportHeight(element.clientHeight || 360)
    measure()
    const resizeObserver = new ResizeObserver(measure)
    resizeObserver.observe(element)
    return () => resizeObserver.disconnect()
  }, [])

  const startFolderRename = useCallback((node: Extract<NotebookTreeNode, { type: 'folder' }>) => {
    setRenameDraft({ type: 'folder', path: node.path, name: node.name })
  }, [])

  const startEntryRename = useCallback((entry: NotebookEntry, name?: string) => {
    const segments = notebookEntryPathSegments(entry)
    setRenameDraft({ type: 'entry', entry, name: name ?? segments[segments.length - 1] ?? entry.title })
  }, [])

  const submitRename = useCallback(async () => {
    if (!renameDraft || committingRenameRef.current) return
    committingRenameRef.current = true
    const succeeded = renameDraft.type === 'folder'
      ? await onRenameFolder(renameDraft.path, renameDraft.name)
      : await onRenameEntry(renameDraft.entry, renameDraft.name)
    committingRenameRef.current = false
    if (succeeded) setRenameDraft(null)
    else window.setTimeout(() => renameInputRef.current?.focus(), 0)
  }, [onRenameEntry, onRenameFolder, renameDraft])

  const openFolderContextMenu = useCallback((event: MouseEvent, node: Extract<NotebookTreeNode, { type: 'folder' }>) => {
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      { label: english ? 'New Note Here' : '在此新建笔记', run: () => onCreateEntry(node.path) },
      { label: english ? 'New Folder Here' : '在此新建目录', run: () => onCreateFolder(node.path) },
      { label: english ? 'Rename Folder' : '重命名目录', run: () => startFolderRename(node) },
      { label: english ? 'Copy Folder Path' : '复制目录路径', run: () => { void writeClipboardText(node.path) } },
      { label: english ? 'Delete Folder' : '删除目录', run: () => onDeleteFolder(node.path) },
    ])
    if (opened) event.preventDefault()
  }, [english, onCreateEntry, onCreateFolder, onDeleteFolder, startFolderRename])

  const openEntryContextMenu = useCallback((event: MouseEvent, entry: NotebookEntry) => {
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      { label: english ? 'Open Note' : '打开笔记', run: () => onSelectEntry(entry.id) },
      { label: english ? 'Rename Note' : '重命名笔记', run: () => startEntryRename(entry) },
      { label: english ? 'Copy Note Path' : '复制笔记路径', run: () => { void writeClipboardText(entry.path ?? entry.title) } },
      { label: english ? 'Delete Note' : '删除笔记', run: () => onDeleteEntry(entry) },
    ])
    if (opened) event.preventDefault()
  }, [english, onDeleteEntry, onSelectEntry, startEntryRename])

  const submitFolderDraft = useCallback(() => {
    if (!folderDraft) return
    if (folderDraft.name.trim()) onCommitFolder(folderDraft.parentPath, folderDraft.name)
    else onCancelFolder()
  }, [folderDraft, onCancelFolder, onCommitFolder])

  return (
    <div ref={containerRef} className="h-full overflow-y-auto" onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}>
      <div className="relative" style={{ height: `${totalHeight}px` }}>
        <div className="absolute left-0 right-0 top-0" style={{ transform: `translateY(${startIndex * NOTEBOOK_TREE_ROW_HEIGHT}px)` }}>
          {renderedRows.map((row) => {
            if (row.type === 'folder_draft') {
              return (
                <div
                  key={`folder-draft:${folderDraft?.parentPath ?? ''}`}
                  data-notebook-tree-row="true"
                  className="mx-1 mb-1 flex h-7 w-auto items-center gap-1.5 rounded-md bg-white pr-2 text-sm ring-1 ring-inset ring-blue-200"
                  style={{ paddingLeft: `${26 + row.depth * 14}px` }}
                >
                  <Folder size={15} className="shrink-0 text-blue-500" />
                  <input
                    autoFocus
                    className="h-6 min-w-0 flex-1 border-0 bg-transparent px-1 text-sm text-gray-900 outline-none"
                    value={folderDraft?.name ?? ''}
                    aria-label={english ? 'New folder name' : '新目录名称'}
                    placeholder={english ? 'Folder name' : '目录名称'}
                    onChange={(event) => onFolderDraftNameChange(event.target.value)}
                    onBlur={submitFolderDraft}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault()
                        submitFolderDraft()
                      } else if (event.key === 'Escape') {
                        event.preventDefault()
                        onCancelFolder()
                      }
                    }}
                  />
                </div>
              )
            }

            const node = row.node
            if (node.type === 'folder') {
              const renaming = renameDraft?.type === 'folder' && renameDraft.path === node.path
              return (
                <div key={`folder:${node.path}`} data-notebook-tree-row="true" data-surface-context-menu="true" className="group mx-1 mb-1 flex h-7 w-auto items-center gap-1 rounded-md pr-1 text-sm text-gray-700 transition-colors hover:bg-white" style={{ paddingLeft: `${6 + row.depth * 14}px` }} onContextMenu={(event) => openFolderContextMenu(event, node)}>
                  {renaming ? (
                    <>
                      <ChevronDown size={14} className={`shrink-0 text-gray-400 ${expandedFolders.has(node.path) ? '' : '-rotate-90'}`} />
                      {expandedFolders.has(node.path) ? <FolderOpen size={16} className="shrink-0 text-blue-500" /> : <Folder size={16} className="shrink-0 text-gray-500" />}
                      <input
                        ref={renameInputRef}
                        autoFocus
                        className="h-6 min-w-0 flex-1 rounded border border-blue-300 bg-white px-1 text-sm text-gray-900 outline-none ring-2 ring-blue-100"
                        value={renameDraft.name}
                        aria-label={english ? 'Rename folder' : '重命名目录'}
                        onFocus={(event) => event.currentTarget.select()}
                        onChange={(event) => setRenameDraft({ ...renameDraft, name: event.target.value })}
                        onBlur={() => void submitRename()}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') { event.preventDefault(); void submitRename() }
                          else if (event.key === 'Escape') { event.preventDefault(); setRenameDraft(null) }
                        }}
                      />
                    </>
                  ) : (
                    <>
                      <button className="flex min-w-0 flex-1 items-center gap-1.5 text-left" type="button" onClick={() => onToggleFolder(node.path)}>
                        <ChevronDown size={14} className={`shrink-0 text-gray-400 transition-transform ${expandedFolders.has(node.path) ? '' : '-rotate-90'}`} />
                        {expandedFolders.has(node.path) ? <FolderOpen size={16} className="shrink-0 text-blue-500" /> : <Folder size={16} className="shrink-0 text-gray-500" />}
                        <span className="min-w-0 flex-1 truncate font-medium" onDoubleClick={(event) => { event.preventDefault(); event.stopPropagation(); startFolderRename(node) }}>{node.name}</span>
                      </button>
                      <button className="rounded p-1 text-gray-400 opacity-0 hover:bg-blue-50 hover:text-blue-700 group-hover:opacity-100" type="button" title={english ? 'New note here' : '在此新建笔记'} onClick={() => onCreateEntry(node.path)}><FileText size={13} /></button>
                      <button className="rounded p-1 text-gray-400 opacity-0 hover:bg-blue-50 hover:text-blue-700 group-hover:opacity-100" type="button" title={english ? 'New folder here' : '在此新建目录'} onClick={() => onCreateFolder(node.path)}><Folder size={13} /></button>
                    </>
                  )}
                </div>
              )
            }

            const entry = node.entry
            const renaming = renameDraft?.type === 'entry' && renameDraft.entry.id === entry.id
            return (
              <div key={entry.id} data-notebook-tree-row="true" data-surface-context-menu="true" className={`group mx-1 mb-1 flex h-7 w-auto items-center rounded-md pr-1 text-sm transition-colors ${activeEntryId === entry.id ? 'bg-blue-50/80 ring-1 ring-inset ring-blue-100' : 'hover:bg-white'}`} style={{ paddingLeft: `${26 + row.depth * 14}px` }} onContextMenu={(event) => openEntryContextMenu(event, entry)}>
                {renaming ? (
                  <>
                    <FileText size={15} className="mr-2 shrink-0 text-blue-600" />
                    <input
                      ref={renameInputRef}
                      autoFocus
                      className="h-6 min-w-0 flex-1 rounded border border-blue-300 bg-white px-1 text-sm text-gray-900 outline-none ring-2 ring-blue-100"
                      value={renameDraft.name}
                      aria-label={english ? 'Rename note' : '重命名笔记'}
                      onFocus={(event) => event.currentTarget.select()}
                      onChange={(event) => setRenameDraft({ ...renameDraft, name: event.target.value })}
                      onBlur={() => void submitRename()}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') { event.preventDefault(); void submitRename() }
                        else if (event.key === 'Escape') { event.preventDefault(); setRenameDraft(null) }
                      }}
                    />
                  </>
                ) : (
                  <>
                    <button className="flex min-w-0 flex-1 items-center gap-2 text-left" type="button" title={entry.path ?? entry.title} onClick={() => onSelectEntry(entry.id)}>
                      <FileText size={15} className="shrink-0 text-blue-600" /><span className="min-w-0 flex-1 truncate font-medium text-gray-900" onDoubleClick={(event) => { event.preventDefault(); event.stopPropagation(); startEntryRename(entry, node.name) }}>{node.name}</span>
                    </button>
                    <button className="rounded p-1 text-gray-400 opacity-0 hover:bg-red-50 hover:text-red-600 group-hover:opacity-100" type="button" title={english ? 'Delete note' : '删除笔记'} onClick={() => onDeleteEntry(entry)}><Trash2 size={14} /></button>
                  </>
                )}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

function buildNotebookTree(entries: NotebookEntry[], folderPaths: string[]): NotebookTreeNode[] {
  const root: NotebookTreeNode[] = []
  const folders = new Map<string, Extract<NotebookTreeNode, { type: 'folder' }>>()
  const ensureFolder = (segments: string[]) => {
    let children = root
    let currentPath = ''
    for (const segment of segments) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment
      let folder = folders.get(currentPath)
      if (!folder) {
        folder = { type: 'folder', name: segment, path: currentPath, children: [] }
        folders.set(currentPath, folder)
        children.push(folder)
        sortNotebookTreeNodes(children)
      }
      children = folder.children
    }
    return children
  }
  for (const folderPath of folderPaths) ensureFolder(notebookPathSegments(folderPath))
  for (const entry of entries) {
    const segments = notebookEntryPathSegments(entry)
    const fileName = segments.pop() ?? `${entry.title || 'Untitled note'}.md`
    const children = ensureFolder(segments)
    children.push({ type: 'entry', name: fileName, path: [...segments, fileName].join('/'), entry })
    sortNotebookTreeNodes(children)
  }
  sortNotebookTreeNodes(root)
  return root
}

function collectNotebookFolderPaths(nodes: NotebookTreeNode[]) {
  const paths: string[] = []
  const visit = (items: NotebookTreeNode[]) => items.forEach((item) => {
    if (item.type === 'folder') { paths.push(item.path); visit(item.children) }
  })
  visit(nodes)
  return paths
}

function flattenVisibleNotebookTree(nodes: NotebookTreeNode[], expandedFolders: Set<string>) {
  const rows: FlatNotebookTreeRow[] = []
  const visit = (items: NotebookTreeNode[], depth: number) => items.forEach((item) => {
    rows.push({ node: item, depth })
    if (item.type === 'folder' && expandedFolders.has(item.path)) visit(item.children, depth + 1)
  })
  visit(nodes, 0)
  return rows
}

function loadExpandedNotebookFolders() {
  try {
    const raw = window.localStorage.getItem(NOTEBOOK_EXPANDED_FOLDERS_KEY)
    if (!raw) return new Set<string>()
    const values = JSON.parse(raw)
    return Array.isArray(values) ? new Set(values.filter((value): value is string => typeof value === 'string' && value.trim().length > 0)) : new Set<string>()
  } catch { return new Set<string>() }
}

function saveExpandedNotebookFolders(folders: Set<string>) {
  try { window.localStorage.setItem(NOTEBOOK_EXPANDED_FOLDERS_KEY, JSON.stringify([...folders].sort())) } catch { /* localStorage may be unavailable. */ }
}

function notebookEntryPathSegments(entry: NotebookEntry) {
  return notebookPathSegments(entry.path || `${entry.title || 'Untitled note'}.md`, `${entry.title || 'Untitled note'}.md`)
}

function notebookPathSegments(path: string, fallback = '') {
  const segments = (path || fallback).replace(/\\/g, '/').split('/').map((segment) => segment.trim()).filter((segment) => segment && segment !== '.' && segment !== '..')
  return segments.length === 0 && fallback ? [fallback] : segments
}

function parentFolder(path?: string | null) {
  if (!path) return undefined
  const segments = notebookPathSegments(path)
  segments.pop()
  return segments.join('/') || undefined
}

function replaceNotebookFolderPrefix(path: string, previous: string, next: string) {
  if (path === previous) return next
  return path.startsWith(`${previous}/`) ? `${next}${path.slice(previous.length)}` : path
}

function sortNotebookTreeNodes(nodes: NotebookTreeNode[]) {
  nodes.sort((left, right) => left.type !== right.type ? (left.type === 'folder' ? -1 : 1) : left.name.localeCompare(right.name, undefined, { sensitivity: 'base', numeric: true }))
}

function setsEqual(left: Set<string>, right: Set<string>) {
  if (left.size !== right.size) return false
  for (const item of left) if (!right.has(item)) return false
  return true
}

async function safeJson(response: Response): Promise<Record<string, unknown>> {
  try { return await response.json() as Record<string, unknown> } catch { return {} }
}

function errorMessage(data: Record<string, unknown>, status: number) {
  return typeof data.error === 'string' ? data.error : `HTTP ${status}`
}

const inputClassName = 'w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100'
const compactButtonClassName = 'inline-flex h-9 items-center justify-center gap-1.5 rounded-lg border border-gray-200 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50'
const primaryCompactButtonClassName = 'inline-flex h-9 items-center justify-center gap-1.5 rounded-lg bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400'
const iconButtonClassName = 'inline-flex h-9 w-9 items-center justify-center rounded-lg border border-gray-200 bg-white text-gray-600 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50'
