import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import {
  AlertTriangle,
  BookMarked,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Edit3,
  FileQuestion,
  FileText,
  Folder,
  FolderOpen,
  Link2,
  Network,
  NotebookPen,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  RefreshCw,
  Save,
  Tags,
  Target,
  Trash2,
  UserRound,
  X,
} from 'lucide-react'
import type { QuizQuestion, QuizSession } from '../quizTypes'
import { quizSourceFilters, quizSourceLabel, quizSourceType, type QuizSourceFilter } from '../quizSource'
import { useI18n, type TranslationKey } from '../i18n'
import { openDesktopContextMenu } from '../desktop'
import { writeClipboardText } from '../api'
import { MarkdownMessage, SourceReferences, sourceTargetFromRaw } from './MarkdownMessage'
import type { SourceReference, SourceTarget } from './MarkdownMessage'

type SpaceTab = 'notebook' | 'quiz_bank' | 'student_profile'

interface NotebookEntry {
  id: string
  space_id: string
  entry_type: 'note' | 'research_report' | 'chat_answer' | 'source_snippet' | 'quiz_summary' | 'deep_solve_result'
  title: string
  path?: string | null
  markdown?: string
  metadata?: Record<string, unknown> | null
  source_session_id?: string | null
  source_message_id?: string | null
  created_at: string
  updated_at: string
  file_size?: number | null
  file_modified_ms?: number | null
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

interface NotebookFolder {
  path: string
}

interface NotebookWatchInfo {
  watching?: boolean
  root?: string | null
  last_refreshed_at?: string | null
  last_result?: {
    entries?: number
    folders?: number
    added?: number
    changed?: number
    unchanged?: number
    removed?: number
  } | null
  last_error?: string | null
}

interface MemoryFile {
  path: string
  level: string
  name: string
  markdown: string
}

const notebookTab: { key: SpaceTab; labelKey: TranslationKey; icon: typeof NotebookPen } = {
  key: 'notebook',
  labelKey: 'space.tabs.notebook',
  icon: NotebookPen,
}

const spaceTabs: Array<{ key: SpaceTab; labelKey: TranslationKey; icon: typeof NotebookPen }> = [
  { key: 'quiz_bank', labelKey: 'space.tabs.quizBank', icon: FileQuestion },
  { key: 'student_profile', labelKey: 'space.tabs.studentProfile', icon: UserRound },
]

const allTabs = [notebookTab, ...spaceTabs]

const profileMemoryPaths = ['L3/profile.md', 'L3/recent.md', 'L3/teaching_strategy.md']

type SpaceFocusTarget = Extract<SourceTarget, { type: 'notebook' | 'quiz' }>

export function SpacePage({
  focusTarget,
  onSourceNavigate,
  onStartQuiz,
  mode = 'space',
  embedded = false,
}: {
  focusTarget?: SpaceFocusTarget | null
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  onStartQuiz?: () => void
  mode?: 'space' | 'notebook'
  embedded?: boolean
}) {
  const { t, language } = useI18n()
  const [activeTab, setActiveTab] = useState<SpaceTab>(() => mode === 'notebook' ? 'notebook' : 'quiz_bank')
  const [quizzes, setQuizzes] = useState<QuizSession[]>([])
  const [activeQuizId, setActiveQuizId] = useState<string | null>(null)
  const [quizSourceFilter, setQuizSourceFilter] = useState<QuizSourceFilter>('all')
  const [notebookEntries, setNotebookEntries] = useState<NotebookEntry[]>([])
  const [notebookFolders, setNotebookFolders] = useState<string[]>([])
  const [expandedNotebookFolders, setExpandedNotebookFolders] = useState<Set<string>>(() => loadExpandedNotebookFolders())
  const [activeNotebookId, setActiveNotebookId] = useState<string | null>(null)
  const [activeNotebookDetail, setActiveNotebookDetail] = useState<NotebookEntry | null>(null)
  const [memoryFiles, setMemoryFiles] = useState<MemoryFile[]>([])
  const [editingNotebookId, setEditingNotebookId] = useState<string | null>(null)
  const [recentlyDeletedNote, setRecentlyDeletedNote] = useState<NotebookEntry | null>(null)
  const [editTitle, setEditTitle] = useState('')
  const [editPath, setEditPath] = useState('')
  const [editMarkdown, setEditMarkdown] = useState('')
  const [editingMemoryPath, setEditingMemoryPath] = useState<string | null>(null)
  const [memoryDraft, setMemoryDraft] = useState('')
  const [questionIndex, setQuestionIndex] = useState(0)
  const [status, setStatus] = useState('Ready')
  const [loading, setLoading] = useState(false)
  const [spaceNavCollapsed, setSpaceNavCollapsed] = useState(false)
  const knownNotebookFolderPathsRef = useRef<Set<string> | null>(null)
  const visibleTabs = mode === 'notebook' ? [notebookTab] : spaceTabs

  const filteredQuizzes = useMemo(
    () => quizzes.filter((quiz) => quizSourceFilter === 'all' || quizSourceType(quiz) === quizSourceFilter),
    [quizzes, quizSourceFilter],
  )
  const activeQuiz = filteredQuizzes.find((quiz) => quiz.id === activeQuizId) ?? null
  const activeQuestion = activeQuiz?.questions[questionIndex] ?? null
  const activeNotebookEntry =
    activeNotebookDetail?.id === activeNotebookId
      ? activeNotebookDetail
      : notebookEntries.find((entry) => entry.id === activeNotebookId) ?? null
  const notebookTree = useMemo(() => buildNotebookTree(notebookEntries, notebookFolders), [notebookEntries, notebookFolders])
  const notebookFolderPaths = useMemo(() => collectNotebookFolderPaths(notebookTree), [notebookTree])
  const profile = useMemo(() => buildProfile(quizzes), [quizzes])

  useEffect(() => {
    setActiveQuizId((current) =>
      current && filteredQuizzes.some((quiz) => quiz.id === current)
        ? current
        : filteredQuizzes[0]?.id ?? null,
    )
  }, [filteredQuizzes])

  const refreshQuizzes = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch('/api/quizzes')
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const items = (data.quizzes ?? []) as QuizSession[]
      setQuizzes(items)
      setActiveQuizId((current) => current && items.some((quiz) => quiz.id === current) ? current : items[0]?.id ?? null)
      setStatus(items.length ? 'Quiz records loaded' : 'No quiz records yet')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  const refreshNotebook = useCallback(async (forceVaultRefresh = false) => {
    setLoading(true)
    try {
      const res = await fetch(
        forceVaultRefresh
          ? '/api/notebook/refresh'
          : '/api/notebook/entries?space_id=default',
        forceVaultRefresh ? { method: 'POST' } : undefined,
      )
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const entries = (data.entries ?? []) as NotebookEntry[]
      const watch = (data.watch ?? null) as NotebookWatchInfo | null
      setNotebookFolders(((data.folders ?? []) as string[]).filter(Boolean))
      setNotebookEntries(entries)
      setActiveNotebookDetail((current) => current && entries.some((entry) => entry.id === current.id) ? current : null)
      setActiveNotebookId((current) => current && entries.some((entry) => entry.id === current) ? current : null)
      if (forceVaultRefresh && data.refresh) {
        const refresh = data.refresh as {
          entries?: number
          folders?: number
          added?: number
          changed?: number
          unchanged?: number
          removed?: number
        }
        setStatus(
          `Vault refreshed: ${refresh.entries ?? entries.length} notes, `
          + `${refresh.added ?? 0} added, ${refresh.changed ?? 0} changed, `
          + `${refresh.unchanged ?? 0} skipped, ${refresh.removed ?? 0} removed`,
        )
      } else {
        const watchText = watch?.last_error
          ? `watch error: ${watch.last_error}`
          : watch?.watching
            ? 'watching vault'
            : 'watcher off'
        setStatus(entries.length ? `Notebook index loaded · ${watchText}` : `No notebook entries yet · ${watchText}`)
      }
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  const loadNotebookDetail = useCallback(async (entryId: string) => {
    try {
      const res = await fetch(`/api/notebook/entries/${encodeURIComponent(entryId)}`)
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const entry = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      setActiveNotebookDetail(entry)
      setNotebookEntries((items) => items.map((item) => item.id === entry.id ? { ...item, ...entry } : item))
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    }
  }, [])

  const refreshMemory = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetch('/api/memory/files')
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setMemoryFiles((data.files ?? []) as MemoryFile[])
      setStatus('Memory files loaded')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refreshQuizzes()
    void refreshNotebook()
    void refreshMemory()
  }, [refreshNotebook, refreshQuizzes, refreshMemory])

  useEffect(() => {
    if (notebookFolderPaths.length === 0 && !knownNotebookFolderPathsRef.current) return
    const currentPaths = new Set(notebookFolderPaths)
    const knownPaths = knownNotebookFolderPathsRef.current
    setExpandedNotebookFolders((current) => {
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
    knownNotebookFolderPathsRef.current = currentPaths
  }, [notebookFolderPaths])

  useEffect(() => {
    saveExpandedNotebookFolders(expandedNotebookFolders)
  }, [expandedNotebookFolders])

  useEffect(() => {
    setActiveTab((current) => {
      if (mode === 'notebook') return 'notebook'
      return current === 'notebook' ? 'quiz_bank' : current
    })
  }, [mode])

  useEffect(() => {
    if (!activeNotebookId) {
      setActiveNotebookDetail(null)
      return
    }
    if (activeNotebookDetail?.id === activeNotebookId) return
    void loadNotebookDetail(activeNotebookId)
  }, [activeNotebookDetail?.id, activeNotebookId, loadNotebookDetail])

  useEffect(() => {
    if (focusTarget?.type === 'quiz' && focusTarget.quizId === activeQuizId) return
    setQuestionIndex(0)
  }, [activeQuizId, focusTarget])

  useEffect(() => {
    if (!focusTarget) return
    if (focusTarget.type === 'notebook') {
      const entryId = focusTarget.entryId
      setActiveTab('notebook')
      if (notebookEntries.some((entry) => entry.id === entryId)) {
        setActiveNotebookId(entryId)
        setStatus(`Opened notebook source: ${entryId}`)
      } else if (notebookEntries.length > 0) {
        setStatus(`Notebook source not found: ${entryId}`)
      }
      return
    }

    setActiveTab('quiz_bank')
    if (quizzes.some((quiz) => quiz.id === focusTarget.quizId)) {
      setActiveQuizId(focusTarget.quizId)
      const quiz = quizzes.find((item) => item.id === focusTarget.quizId)
      const nextQuestionIndex = quiz?.questions.findIndex((question) => question.id === focusTarget.questionId) ?? -1
      if (nextQuestionIndex >= 0) {
        setQuestionIndex(nextQuestionIndex)
        setStatus(`Opened quiz source: ${focusTarget.quizId} / ${focusTarget.questionId}`)
      } else {
        setQuestionIndex(0)
        setStatus(`Opened quiz source: ${focusTarget.quizId}`)
      }
    } else if (quizzes.length > 0) {
      setStatus(`Quiz source not found: ${focusTarget.quizId}`)
    }
  }, [focusTarget, notebookEntries, quizzes])

  const refreshActiveTab = () => {
    if (activeTab === 'notebook') void refreshNotebook(true)
    else if (activeTab === 'quiz_bank') void refreshQuizzes()
    else void refreshMemory()
  }

  const toggleNotebookFolder = useCallback((folderPath: string) => {
    setExpandedNotebookFolders((current) => {
      const next = new Set(current)
      if (next.has(folderPath)) next.delete(folderPath)
      else next.add(folderPath)
      return next
    })
  }, [])

  const expandNotebookFolderPath = useCallback((folderPath?: string) => {
    if (!folderPath) return
    const normalized = folderPath.replace(/\/+$/, '')
    if (!normalized) return
    setExpandedNotebookFolders((current) => {
      const next = new Set(current)
      const segments = normalized.split('/').filter(Boolean)
      for (let index = 0; index < segments.length; index += 1) {
        next.add(segments.slice(0, index + 1).join('/'))
      }
      return next
    })
  }, [])

  const createNotebookEntry = async (folderPath?: string) => {
    const title = 'Untitled note'
    const markdown = '# Untitled note\n\n'
    const path = folderPath ? `${folderPath.replace(/\/+$/, '')}/${title}.md` : undefined
    setLoading(true)
    try {
      const res = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: 'default',
          entry_type: 'note',
          title,
          path,
          markdown,
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const entry = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      expandNotebookFolderPath(folderPath)
      setNotebookEntries((items) => [entry, ...items.filter((item) => item.id !== entry.id)])
      setActiveNotebookId(entry.id)
      setStatus('Notebook entry created')
      startEditNotebookEntry(entry)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const createNotebookFolder = async (parentPath?: string) => {
    const name = window.prompt('Folder name')
    if (!name?.trim()) return
    const path = parentPath ? `${parentPath.replace(/\/+$/, '')}/${name.trim()}` : name.trim()
    setLoading(true)
    try {
      const res = await fetch('/api/notebook/folders', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookFolders(((data.folders ?? []) as string[]).filter(Boolean))
      expandNotebookFolderPath((data.folder as NotebookFolder | undefined)?.path ?? path)
      setStatus(`Created folder: ${(data.folder as NotebookFolder | undefined)?.path ?? path}`)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const createNotebookEntryFromLink = async (title: string) => {
    const cleanTitle = title.trim() || 'Untitled note'
    setLoading(true)
    try {
      const res = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: 'default',
          entry_type: 'note',
          title: cleanTitle,
          markdown: `# ${cleanTitle}\n\n`,
          metadata: {
            created_from_unresolved_link: true,
          },
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const entry = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      setNotebookEntries((items) => [entry, ...items.filter((item) => item.id !== entry.id)])
      setActiveNotebookId(entry.id)
      setStatus(`Created linked note: ${entry.title}`)
      startEditNotebookEntry(entry)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const deleteNotebookEntry = async (entry: NotebookEntry) => {
    if (!window.confirm(`Delete "${entry.title}"?`)) return
    const previous = notebookEntries
    setNotebookEntries((items) => items.filter((item) => item.id !== entry.id))
    setActiveNotebookId((current) => current === entry.id ? null : current)
    try {
      let restorable = activeNotebookDetail?.id === entry.id ? activeNotebookDetail : entry
      if (restorable.markdown === undefined) {
        const detailResponse = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`)
        const detailData = await safeJson(detailResponse)
        if (!detailResponse.ok) throw new Error(errorMessage(detailData, detailResponse.status))
        restorable = { ...(detailData.entry as NotebookEntry), revision: detailData.revision as string }
      }
      const res = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`, { method: 'DELETE' })
      if (!res.ok) {
        const data = await safeJson(res)
        throw new Error(errorMessage(data, res.status))
      }
      setRecentlyDeletedNote(restorable)
      setStatus('Notebook entry deleted')
    } catch (err) {
      setNotebookEntries(previous)
      setStatus(err instanceof Error ? err.message : String(err))
    }
  }

  const restoreRecentlyDeletedNote = async () => {
    if (!recentlyDeletedNote) return
    setLoading(true)
    try {
      const res = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: recentlyDeletedNote.space_id,
          entry_type: recentlyDeletedNote.entry_type,
          title: recentlyDeletedNote.title,
          path: recentlyDeletedNote.path,
          markdown: recentlyDeletedNote.markdown ?? `# ${recentlyDeletedNote.title}\n`,
          metadata: recentlyDeletedNote.metadata,
          source_session_id: recentlyDeletedNote.source_session_id,
          source_message_id: recentlyDeletedNote.source_message_id,
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const restored = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      setNotebookEntries((items) => [restored, ...items])
      setActiveNotebookId(restored.id)
      setRecentlyDeletedNote(null)
      setStatus('Notebook entry restored')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const startEditNotebookEntry = (entry: NotebookEntry) => {
    setEditingNotebookId(entry.id)
    setEditTitle(entry.title)
    setEditPath(entry.path ?? '')
    setEditMarkdown(entry.markdown ?? '')
  }

  const cancelEditNotebookEntry = () => {
    setEditingNotebookId(null)
    setEditTitle('')
    setEditPath('')
    setEditMarkdown('')
  }

  const saveNotebookEntry = async (entry: NotebookEntry) => {
    const title = editTitle.trim() || 'Untitled note'
    const markdown = editMarkdown.trim()
    if (!markdown) {
      setStatus('Notebook markdown is empty')
      return
    }
    setLoading(true)
    try {
      let revision = entry.revision
      if (!revision) {
        const detailRes = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`)
        const detailData = await safeJson(detailRes)
        if (!detailRes.ok) throw new Error(errorMessage(detailData, detailRes.status))
        revision = detailData.revision as string
      }
      const res = await fetch(`/api/notebook/entries/${encodeURIComponent(entry.id)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_revision: revision,
          title,
          path: editPath.trim() || `${title}.md`,
          markdown,
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const updated = { ...(data.entry as NotebookEntry), revision: data.revision as string }
      setNotebookEntries((items) => items.map((item) => item.id === updated.id ? updated : item))
      setActiveNotebookDetail(updated)
      setEditingNotebookId(null)
      setStatus('Notebook entry updated')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const deleteQuiz = async (quiz: QuizSession) => {
    if (!window.confirm(`Delete "${quiz.title}"?`)) return
    const previous = quizzes
    setQuizzes((items) => items.filter((item) => item.id !== quiz.id))
    setActiveQuizId((current) => current === quiz.id ? null : current)
    try {
      const res = await fetch(`/api/quizzes/${encodeURIComponent(quiz.id)}`, { method: 'DELETE' })
      if (!res.ok) {
        const data = await safeJson(res)
        throw new Error(errorMessage(data, res.status))
      }
      setStatus('Quiz deleted')
    } catch (err) {
      setQuizzes(previous)
      setStatus(err instanceof Error ? err.message : String(err))
    }
  }

  const startEditMemory = (file: MemoryFile) => {
    setEditingMemoryPath(file.path)
    setMemoryDraft(file.markdown)
  }

  const cancelEditMemory = () => {
    setEditingMemoryPath(null)
    setMemoryDraft('')
  }

  const saveMemoryFile = async (path: string) => {
    if (!memoryDraft.trim()) {
      setStatus('Memory markdown is empty')
      return
    }
    setLoading(true)
    try {
      const res = await fetch(`/api/memory/file?path=${encodeURIComponent(path)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ markdown: memoryDraft }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const updated = data.file as MemoryFile
      setMemoryFiles((items) => items.map((item) => item.path === updated.path ? updated : item))
      setEditingMemoryPath(null)
      setMemoryDraft('')
      setStatus('Memory file saved')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  if (mode === 'notebook') {
    return (
      <main className="flex h-full min-h-0 flex-col bg-white">
        {!embedded && <header className="flex items-center gap-4 border-b border-gray-100 px-8 py-5">
          <div>
            <h1 className="text-xl font-semibold text-gray-950">{t('space.tabs.notebook')}</h1>
            <p className="mt-1 text-sm text-gray-500">{subtitleFor('notebook', t)}</p>
          </div>
          <button
            className="ml-auto inline-flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50"
            type="button"
            disabled={loading}
            onClick={refreshActiveTab}
          >
            <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
            {t('space.refresh')}
          </button>
        </header>}

        {recentlyDeletedNote && (
          <div className="flex items-center gap-3 border-b border-amber-100 bg-amber-50 px-6 py-2 text-sm text-amber-900">
            <span className="min-w-0 flex-1 truncate">{language === 'en-US' ? `Deleted “${recentlyDeletedNote.title}”` : `已删除“${recentlyDeletedNote.title}”`}</span>
            <button type="button" className="rounded-md bg-white px-3 py-1.5 font-medium text-amber-800 shadow-sm" disabled={loading} onClick={() => void restoreRecentlyDeletedNote()}>{language === 'en-US' ? 'Undo' : '撤销'}</button>
            <button type="button" className="rounded p-1 text-amber-700 hover:bg-amber-100" aria-label="Dismiss undo" onClick={() => setRecentlyDeletedNote(null)}><X size={15} /></button>
          </div>
        )}

        <NotebookTab
          entries={notebookEntries}
          folders={notebookFolders}
          expandedFolders={expandedNotebookFolders}
          activeEntry={activeNotebookEntry}
          status={status}
          loading={loading}
          editingEntryId={editingNotebookId}
          editTitle={editTitle}
          editPath={editPath}
          editMarkdown={editMarkdown}
          onSelectEntry={setActiveNotebookId}
          onToggleFolder={toggleNotebookFolder}
          onCreateEntry={(folderPath) => void createNotebookEntry(folderPath)}
          onCreateFolder={(parentPath) => void createNotebookFolder(parentPath)}
          onDeleteEntry={(entry) => void deleteNotebookEntry(entry)}
          onStartEdit={startEditNotebookEntry}
          onCancelEdit={cancelEditNotebookEntry}
          onEditTitleChange={setEditTitle}
          onEditPathChange={setEditPath}
          onEditMarkdownChange={setEditMarkdown}
          onSaveEntry={(entry) => void saveNotebookEntry(entry)}
          onCreateLinkedEntry={(title) => void createNotebookEntryFromLink(title)}
          onSourceNavigate={onSourceNavigate}
          language={language}
        />
      </main>
    )
  }

  return (
    <main className="flex h-full min-h-0 bg-white">
      <aside className={`flex shrink-0 flex-col border-r border-gray-200 bg-gray-50 transition-[width] duration-200 ${spaceNavCollapsed ? 'w-16' : 'w-72'}`}>
        <div className={`${spaceNavCollapsed ? 'flex justify-center px-2 py-4' : 'px-5 py-5'}`}>
          <div className={`flex gap-3 ${spaceNavCollapsed ? 'items-center justify-center' : 'items-start justify-between'}`}>
            {!spaceNavCollapsed && (
              <div className="min-w-0">
                <div className="text-xs font-medium uppercase tracking-wide text-blue-600">{t('space.default')}</div>
                <h1 className="mt-1 text-2xl font-semibold text-gray-950">{t('space.title')}</h1>
              </div>
            )}
            <button
              className={`shrink-0 rounded text-gray-500 hover:bg-white hover:text-blue-700 ${spaceNavCollapsed ? 'p-2' : 'inline-flex h-9 w-9 items-center justify-center rounded-lg'}`}
              type="button"
              title={spaceNavCollapsed ? t('space.expand') : t('space.collapse')}
              aria-label={spaceNavCollapsed ? t('space.expand') : t('space.collapse')}
              onClick={() => setSpaceNavCollapsed((value) => !value)}
            >
              {spaceNavCollapsed ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
            </button>
          </div>
          {!spaceNavCollapsed && (
            <p className="mt-2 text-sm leading-6 text-gray-500">
              {t('space.description')}
            </p>
          )}
        </div>

        <nav className={`space-y-1 ${spaceNavCollapsed ? 'px-2' : 'px-3'}`}>
          {visibleTabs.map((tab) => {
            const Icon = tab.icon
            const active = activeTab === tab.key
            const label = t(tab.labelKey)
            return (
              <button
                key={tab.key}
                className={`flex w-full items-center rounded-lg py-2.5 text-left text-sm font-medium ${
                  active ? 'bg-white text-blue-700 shadow-sm ring-1 ring-blue-100' : 'text-gray-700 hover:bg-white hover:text-gray-950'
                } ${spaceNavCollapsed ? 'justify-center px-0' : 'gap-3 px-3'}`}
                type="button"
                title={label}
                aria-label={label}
                onClick={() => setActiveTab(tab.key)}
              >
                <Icon size={18} />
                {!spaceNavCollapsed && <span>{label}</span>}
              </button>
            )
          })}
        </nav>

        <div className={`mt-auto border-t border-gray-200 px-5 py-4 text-xs text-gray-500 ${spaceNavCollapsed ? 'hidden' : ''}`}>
          {t('space.localFirst')}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-4 border-b border-gray-100 px-8 py-5">
          <div>
            <h2 className="text-xl font-semibold text-gray-950">
              {t(allTabs.find((tab) => tab.key === activeTab)?.labelKey ?? 'space.tabs.quizBank')}
            </h2>
            <p className="mt-1 text-sm text-gray-500">{subtitleFor(activeTab, t)}</p>
          </div>
          <button
            className="ml-auto inline-flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50"
            type="button"
            disabled={loading}
            onClick={refreshActiveTab}
          >
            <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
            {t('space.refresh')}
          </button>
        </header>

        {activeTab === 'notebook' && (
          <NotebookTab
            entries={notebookEntries}
            folders={notebookFolders}
            expandedFolders={expandedNotebookFolders}
            activeEntry={activeNotebookEntry}
            status={status}
            loading={loading}
            editingEntryId={editingNotebookId}
            editTitle={editTitle}
            editPath={editPath}
            editMarkdown={editMarkdown}
            onSelectEntry={setActiveNotebookId}
            onToggleFolder={toggleNotebookFolder}
            onCreateEntry={(folderPath) => void createNotebookEntry(folderPath)}
            onCreateFolder={(parentPath) => void createNotebookFolder(parentPath)}
            onDeleteEntry={(entry) => void deleteNotebookEntry(entry)}
            onStartEdit={startEditNotebookEntry}
            onCancelEdit={cancelEditNotebookEntry}
            onEditTitleChange={setEditTitle}
            onEditPathChange={setEditPath}
            onEditMarkdownChange={setEditMarkdown}
            onSaveEntry={(entry) => void saveNotebookEntry(entry)}
            onCreateLinkedEntry={(title) => void createNotebookEntryFromLink(title)}
            onSourceNavigate={onSourceNavigate}
            language={language}
          />
        )}
        {activeTab === 'quiz_bank' && (
          <QuizBankTab
            quizzes={quizzes}
            filteredQuizzes={filteredQuizzes}
            sourceFilter={quizSourceFilter}
            onSourceFilterChange={setQuizSourceFilter}
            activeQuiz={activeQuiz}
            activeQuestion={activeQuestion}
            questionIndex={questionIndex}
            status={status}
            onSelectQuiz={setActiveQuizId}
            onDeleteQuiz={(quiz) => void deleteQuiz(quiz)}
            onQuestionIndexChange={setQuestionIndex}
            onSourceNavigate={onSourceNavigate}
            onStartQuiz={onStartQuiz}
            language={language}
          />
        )}
        {activeTab === 'student_profile' && (
          <StudentProfileTab
            profile={profile}
            memoryFiles={memoryFiles}
            editingMemoryPath={editingMemoryPath}
            memoryDraft={memoryDraft}
            loading={loading}
            onStartEditMemory={startEditMemory}
            onCancelEditMemory={cancelEditMemory}
            onMemoryDraftChange={setMemoryDraft}
            onSaveMemoryFile={(path) => void saveMemoryFile(path)}
            onSourceNavigate={onSourceNavigate}
          />
        )}
      </section>
    </main>
  )
}

function NotebookTab({
  entries,
  folders,
  expandedFolders,
  activeEntry,
  status,
  loading,
  editingEntryId,
  editTitle,
  editPath,
  editMarkdown,
  onSelectEntry,
  onToggleFolder,
  onCreateEntry,
  onCreateFolder,
  onDeleteEntry,
  onStartEdit,
  onCancelEdit,
  onEditTitleChange,
  onEditPathChange,
  onEditMarkdownChange,
  onSaveEntry,
  onCreateLinkedEntry,
  onSourceNavigate,
  language,
}: {
  entries: NotebookEntry[]
  folders: string[]
  expandedFolders: Set<string>
  activeEntry: NotebookEntry | null
  status: string
  loading: boolean
  editingEntryId: string | null
  editTitle: string
  editPath: string
  editMarkdown: string
  onSelectEntry: (id: string) => void
  onToggleFolder: (folderPath: string) => void
  onCreateEntry: (folderPath?: string) => void
  onCreateFolder: (parentPath?: string) => void
  onDeleteEntry: (entry: NotebookEntry) => void
  onStartEdit: (entry: NotebookEntry) => void
  onCancelEdit: () => void
  onEditTitleChange: (value: string) => void
  onEditPathChange: (value: string) => void
  onEditMarkdownChange: (value: string) => void
  onSaveEntry: (entry: NotebookEntry) => void
  onCreateLinkedEntry: (title: string) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  language: 'zh-CN' | 'en-US'
}) {
  const isEditing = activeEntry ? editingEntryId === activeEntry.id : false
  const [relationsCollapsed, setRelationsCollapsed] = useState(true)
  const notebookTree = useMemo(() => buildNotebookTree(entries, folders), [entries, folders])

  const wikiLinkResolver = useCallback((target: string): SourceReference | undefined => {
    if (!activeEntry) return undefined
    const normalized = target.trim().toLowerCase()
    const link = (activeEntry.links ?? []).find((item) => {
      return item.target.trim().toLowerCase() === normalized
        || item.target_title?.trim().toLowerCase() === normalized
        || item.target_id === target
    })
    const title = link?.alias || link?.target_title || link?.target || target
    if (link?.target_id) {
      return {
        id: `wiki:${activeEntry.id}:${target}`,
        label: title,
        raw: `notebook:${link.target_id}`,
        surface: 'notebook',
        title,
        target: { type: 'notebook', entryId: link.target_id },
      }
    }
    return {
      id: `wiki:${activeEntry.id}:${target}`,
      label: title,
      raw: `notebook:${target}`,
      surface: 'notebook',
      title,
      metadata: { missingReason: 'Create linked notebook entry' },
    }
  }, [activeEntry])

  return (
    <div className="flex min-h-0 flex-1">
      <aside className="flex w-80 shrink-0 flex-col border-r border-gray-100 bg-gray-50/70">
        <div className="space-y-3 border-b border-gray-100 p-4">
          <div className="grid grid-cols-2 gap-1.5">
            <button
              className="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-blue-600 px-2 text-xs font-medium text-white hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400"
              type="button"
              disabled={loading}
              onClick={() => onCreateEntry()}
            >
              <Plus size={14} />
              New
            </button>
            <button
              className="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg border border-gray-200 bg-white px-2 text-xs font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:bg-gray-100 disabled:text-gray-400"
              type="button"
              disabled={loading}
              onClick={() => onCreateFolder()}
            >
              <Folder size={14} />
              Folder
            </button>
          </div>
          <div className="text-xs text-gray-500">{status}</div>
        </div>

        <div className="min-h-0 flex-1 px-3 py-4">
          {entries.length === 0 ? (
            <div className="rounded-lg px-3 py-8 text-center text-sm text-gray-400">No notebook entries yet</div>
          ) : (
            <NotebookFileTree
              nodes={notebookTree}
              activeEntryId={activeEntry?.id ?? null}
              expandedFolders={expandedFolders}
              onToggleFolder={onToggleFolder}
              onSelectEntry={onSelectEntry}
              onCreateEntry={onCreateEntry}
              onCreateFolder={onCreateFolder}
              onDeleteEntry={onDeleteEntry}
            />
          )}
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        {!activeEntry ? (
          <EmptyDetail
            icon={BookMarked}
            title={language === 'en-US' ? 'Create or select a note' : '创建或选择一份笔记'}
            description={language === 'en-US'
              ? 'Notebook stores research reports, useful chat answers, source snippets, and quiz summaries as Markdown entries.'
              : 'Notebook 使用 Markdown 保存研究报告、重要回答、资料片段和测验总结。'}
            actionLabel={language === 'en-US' ? 'New note' : '新建笔记'}
            onAction={() => onCreateEntry()}
          />
        ) : (
          <>
            <div className="flex items-start gap-4 border-b border-gray-100 px-8 py-5">
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium uppercase tracking-wide text-blue-600">{activeEntry.entry_type.replaceAll('_', ' ')}</div>
                {isEditing ? (
                  <div className="mt-2 grid max-w-2xl gap-2">
                    <input
                      className={`${inputClassName} text-base font-semibold`}
                      value={editTitle}
                      aria-label="Note title"
                      onChange={(event) => onEditTitleChange(event.target.value)}
                    />
                    <input
                      className={`${inputClassName} font-mono text-xs`}
                      value={editPath}
                      aria-label="Note path"
                      placeholder="folder/note.md"
                      onChange={(event) => onEditPathChange(event.target.value)}
                    />
                  </div>
                ) : (
                  <h3 className="mt-1 truncate text-xl font-semibold text-gray-950">{activeEntry.title}</h3>
                )}
              </div>
              {isEditing ? (
                <div className="flex shrink-0 items-center gap-2">
                  <button
                    className={secondaryButtonClassName}
                    type="button"
                    disabled={loading || !editMarkdown.trim()}
                    onClick={() => onSaveEntry(activeEntry)}
                  >
                    <Save size={16} />
                    Save
                  </button>
                  <button className={secondaryButtonClassName} type="button" disabled={loading} onClick={onCancelEdit}>
                    <X size={16} />
                    Cancel
                  </button>
                </div>
              ) : (
                <div className="flex shrink-0 items-center gap-2">
                  <button className={secondaryButtonClassName} type="button" disabled={loading} onClick={() => onStartEdit(activeEntry)}>
                    <Edit3 size={16} />
                    Edit
                  </button>
                </div>
              )}
            </div>
            <div className="flex min-h-0 flex-1 overflow-hidden">
              <div className="min-w-0 flex-1 overflow-y-auto px-8 py-6">
              {isEditing ? (
                <textarea
                  className={`${inputClassName} min-h-[520px] max-w-4xl resize-y font-mono leading-6`}
                  value={editMarkdown}
                  onChange={(event) => onEditMarkdownChange(event.target.value)}
                />
              ) : (
                <div className="max-w-4xl rounded-lg border border-gray-200 bg-gray-50 p-5">
                  <MarkdownMessage
                    text={activeEntry.markdown || ' '}
                    onSourceNavigate={onSourceNavigate}
                    wikiLinkResolver={wikiLinkResolver}
                    onWikiLinkCreate={onCreateLinkedEntry}
                  />
                </div>
              )}
              </div>
              {!isEditing && (
                <NotebookRelationsPanel
                  entry={activeEntry}
                  collapsed={relationsCollapsed}
                  onCollapsedChange={setRelationsCollapsed}
                  onSelectEntry={onSelectEntry}
                  onCreateLinkedEntry={onCreateLinkedEntry}
                />
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}

function NotebookRelationsPanel({
  entry,
  collapsed,
  onCollapsedChange,
  onSelectEntry,
  onCreateLinkedEntry,
}: {
  entry: NotebookEntry
  collapsed: boolean
  onCollapsedChange: (collapsed: boolean) => void
  onSelectEntry: (id: string) => void
  onCreateLinkedEntry: (title: string) => void
}) {
  const tags = entry.tags ?? []
  const links = entry.links ?? []
  const backlinks = entry.backlinks ?? []
  const unresolvedLinks = links.filter((link) => !link.resolved)
  const [localGraphCollapsed, setLocalGraphCollapsed] = useState(false)

  if (collapsed) {
    return (
      <aside className="hidden w-12 shrink-0 border-l border-gray-100 bg-white px-1.5 py-5 xl:flex xl:flex-col xl:items-center">
        <button
          className="inline-flex h-9 w-9 items-center justify-center rounded-lg text-gray-500 hover:bg-blue-50 hover:text-blue-700"
          type="button"
          title="Expand related information"
          aria-label="Expand related information"
          onClick={() => onCollapsedChange(false)}
        >
          <PanelRightOpen size={18} />
        </button>
        <div className="mt-4 rotate-90 whitespace-nowrap text-xs font-semibold uppercase tracking-wide text-gray-400">
          Related
        </div>
      </aside>
    )
  }

  return (
    <aside className="hidden w-80 shrink-0 overflow-y-auto border-l border-gray-100 bg-white px-4 py-5 xl:block">
      <div className="space-y-6">
        <div className="flex items-center justify-between gap-3">
          <div className="text-xs font-semibold uppercase tracking-wide text-gray-500">Related information</div>
          <button
            className="inline-flex h-9 w-9 items-center justify-center rounded-lg text-gray-500 hover:bg-blue-50 hover:text-blue-700"
            type="button"
            title="Collapse related information"
            aria-label="Collapse related information"
            onClick={() => onCollapsedChange(true)}
          >
            <PanelRightClose size={16} />
          </button>
        </div>
        <NotebookLocalGraph
          entry={entry}
          links={links}
          backlinks={backlinks}
          collapsed={localGraphCollapsed}
          onCollapsedChange={setLocalGraphCollapsed}
          onSelectEntry={onSelectEntry}
          onCreateLinkedEntry={onCreateLinkedEntry}
        />

        <section>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <AlertTriangle size={14} />
            Unresolved links
          </div>
          {unresolvedLinks.length === 0 ? (
            <div className="text-sm text-gray-400">All outgoing links resolve</div>
          ) : (
            <div className="space-y-2">
              {unresolvedLinks.map((link) => (
                <button
                  key={`unresolved-${link.raw}-${link.target}`}
                  className="w-full rounded-lg border border-dashed border-amber-200 bg-amber-50 px-3 py-2 text-left text-sm text-amber-800 transition hover:bg-amber-100"
                  type="button"
                  onClick={() => onCreateLinkedEntry(link.target)}
                >
                  <span className="block truncate font-medium">{link.alias || link.target}</span>
                  <span className="mt-0.5 block truncate text-xs text-amber-700">Create note from {link.raw}</span>
                </button>
              ))}
            </div>
          )}
        </section>

        <section>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <Tags size={14} />
            Tags
          </div>
          {tags.length === 0 ? (
            <div className="text-sm text-gray-400">No tags yet</div>
          ) : (
            <div className="flex flex-wrap gap-2">
              {tags.map((tag) => (
                <span key={tag} className="rounded-full bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700">
                  #{tag}
                </span>
              ))}
            </div>
          )}
        </section>

        <section>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <Link2 size={14} />
            Links
          </div>
          {links.length === 0 ? (
            <div className="text-sm text-gray-400">No outgoing links</div>
          ) : (
            <div className="space-y-2">
              {links.map((link) => (
                <button
                  key={`${link.raw}-${link.target_id ?? link.target}`}
                  className={`w-full rounded-lg border px-3 py-2 text-left text-sm transition ${
                    link.resolved
                      ? 'border-blue-100 bg-blue-50/60 text-blue-800 hover:bg-blue-100'
                      : 'border-dashed border-amber-200 bg-amber-50/70 text-amber-700 hover:bg-amber-100'
                  }`}
                  type="button"
                  onClick={() => {
                    if (link.target_id) {
                      onSelectEntry(link.target_id)
                    } else {
                      onCreateLinkedEntry(link.target)
                    }
                  }}
                >
                  <span className="block truncate font-medium">
                    {link.alias || link.target_title || link.target}
                  </span>
                  <span className="mt-0.5 block truncate text-xs opacity-75">
                    {link.resolved ? 'resolved note' : 'create note'}
                  </span>
                </button>
              ))}
            </div>
          )}
        </section>

        <section>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <NotebookPen size={14} />
            Backlinks
          </div>
          {backlinks.length === 0 ? (
            <div className="text-sm text-gray-400">No backlinks yet</div>
          ) : (
            <div className="space-y-2">
              {backlinks.map((backlink) => (
                <button
                  key={`${backlink.source_entry_id}-${backlink.raw}`}
                  className="w-full rounded-lg border border-gray-100 bg-gray-50 px-3 py-2 text-left text-sm text-gray-700 transition hover:border-blue-100 hover:bg-blue-50"
                  type="button"
                  onClick={() => onSelectEntry(backlink.source_entry_id)}
                >
                  <span className="block truncate font-medium text-gray-900">{backlink.source_title}</span>
                  <span className="mt-1 line-clamp-2 text-xs leading-5 text-gray-500">{backlink.snippet}</span>
                </button>
              ))}
            </div>
          )}
        </section>
      </div>
    </aside>
  )
}

type NotebookTreeNode =
  | {
      type: 'folder'
      name: string
      path: string
      children: NotebookTreeNode[]
    }
  | {
      type: 'entry'
      name: string
      path: string
      entry: NotebookEntry
    }

type FlatNotebookTreeRow = {
  node: NotebookTreeNode
  depth: number
}

const NOTEBOOK_TREE_ROW_HEIGHT = 32
const NOTEBOOK_TREE_OVERSCAN_ROWS = 12
const NOTEBOOK_EXPANDED_FOLDERS_KEY = 'llm-tutor:notebook-expanded-folders:v2'

function NotebookFileTree({
  nodes,
  activeEntryId,
  expandedFolders,
  onToggleFolder,
  onSelectEntry,
  onCreateEntry,
  onCreateFolder,
  onDeleteEntry,
}: {
  nodes: NotebookTreeNode[]
  activeEntryId: string | null
  expandedFolders: Set<string>
  onToggleFolder: (folderPath: string) => void
  onSelectEntry: (id: string) => void
  onCreateEntry: (folderPath?: string) => void
  onCreateFolder: (parentPath?: string) => void
  onDeleteEntry: (entry: NotebookEntry) => void
}) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const [scrollTop, setScrollTop] = useState(0)
  const [viewportHeight, setViewportHeight] = useState(360)
  const visibleRows = useMemo(() => flattenVisibleNotebookTree(nodes, expandedFolders), [nodes, expandedFolders])
  const totalHeight = visibleRows.length * NOTEBOOK_TREE_ROW_HEIGHT
  const startIndex = Math.max(0, Math.floor(scrollTop / NOTEBOOK_TREE_ROW_HEIGHT) - NOTEBOOK_TREE_OVERSCAN_ROWS)
  const endIndex = Math.min(
    visibleRows.length,
    Math.ceil((scrollTop + viewportHeight) / NOTEBOOK_TREE_ROW_HEIGHT) + NOTEBOOK_TREE_OVERSCAN_ROWS,
  )
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

  const openFolderContextMenu = useCallback((event: MouseEvent, node: Extract<NotebookTreeNode, { type: 'folder' }>) => {
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      {
        label: expandedFolders.has(node.path) ? 'Collapse Folder' : 'Expand Folder',
        run: () => onToggleFolder(node.path),
      },
      {
        label: 'New Note Here',
        run: () => onCreateEntry(node.path),
      },
      {
        label: 'New Folder Here',
        run: () => onCreateFolder(node.path),
      },
      {
        label: 'Copy Folder Path',
        run: () => {
          void writeClipboardText(node.path)
        },
      },
    ])
    if (opened) event.preventDefault()
  }, [expandedFolders, onCreateEntry, onCreateFolder, onToggleFolder])

  const openEntryContextMenu = useCallback((event: MouseEvent, entry: NotebookEntry) => {
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      {
        label: 'Open Note',
        run: () => onSelectEntry(entry.id),
      },
      {
        label: 'Copy Note Path',
        run: () => {
          void writeClipboardText(entry.path ?? entry.title)
        },
      },
      {
        label: 'Delete Note',
        run: () => onDeleteEntry(entry),
      },
    ])
    if (opened) event.preventDefault()
  }, [onDeleteEntry, onSelectEntry])

  return (
    <div
      ref={containerRef}
      className="h-full overflow-y-auto"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div className="relative" style={{ height: `${totalHeight}px` }}>
        <div
          className="absolute left-0 right-0 top-0"
          style={{ transform: `translateY(${startIndex * NOTEBOOK_TREE_ROW_HEIGHT}px)` }}
        >
          {renderedRows.map(({ node, depth }) => {
            if (node.type === 'folder') {
              const expanded = expandedFolders.has(node.path)
              return (
                <div
                  key={`folder:${node.path}`}
                  data-surface-context-menu="true"
                  className="group flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-sm text-gray-700 hover:bg-white"
                  style={{ paddingLeft: `${8 + depth * 14}px` }}
                  onContextMenu={(event) => openFolderContextMenu(event, node)}
                >
                  <button className="flex min-w-0 flex-1 items-center gap-1.5 text-left" type="button" onClick={() => onToggleFolder(node.path)}>
                    <ChevronDown
                      size={14}
                      className={`shrink-0 text-gray-400 transition-transform ${expanded ? '' : '-rotate-90'}`}
                    />
                    {expanded ? (
                      <FolderOpen size={16} className="shrink-0 text-blue-500" />
                    ) : (
                      <Folder size={16} className="shrink-0 text-gray-500" />
                    )}
                    <span className="min-w-0 flex-1 truncate font-medium">{node.name}</span>
                  </button>
                  <button
                    className="rounded p-1 text-gray-400 opacity-0 hover:bg-blue-50 hover:text-blue-700 group-hover:opacity-100"
                    type="button"
                    title="New file here"
                    onClick={() => onCreateEntry(node.path)}
                  >
                    <FileText size={14} />
                  </button>
                  <button
                    className="rounded p-1 text-gray-400 opacity-0 hover:bg-blue-50 hover:text-blue-700 group-hover:opacity-100"
                    type="button"
                    title="New folder here"
                    onClick={() => onCreateFolder(node.path)}
                  >
                    <Folder size={14} />
                  </button>
                </div>
              )
            }

            const entry = node.entry
            const active = activeEntryId === entry.id
            return (
              <button
                key={entry.id}
                data-surface-context-menu="true"
                className={`group flex h-8 w-full items-center gap-2 rounded-md pr-2 text-left text-sm ${
                  active ? 'bg-white shadow-sm ring-1 ring-blue-100' : 'hover:bg-white'
                }`}
                style={{ paddingLeft: `${28 + depth * 14}px` }}
                type="button"
                title={entry.path ?? entry.title}
                onContextMenu={(event) => openEntryContextMenu(event, entry)}
                onClick={() => onSelectEntry(entry.id)}
              >
                <FileText size={16} className="shrink-0 text-blue-600" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium text-gray-900">{entry.title}</span>
                </span>
                <span
                  role="button"
                  tabIndex={0}
                  className="rounded p-1 text-gray-400 opacity-0 hover:bg-red-50 hover:text-red-600 group-hover:opacity-100"
                  onClick={(event) => {
                    event.stopPropagation()
                    onDeleteEntry(entry)
                  }}
                >
                  <Trash2 size={15} />
                </span>
              </button>
            )
          })}
        </div>
      </div>
    </div>
  )
}

function NotebookLocalGraph({
  entry,
  links,
  backlinks,
  collapsed,
  onCollapsedChange,
  onSelectEntry,
  onCreateLinkedEntry,
}: {
  entry: NotebookEntry
  links: NotebookLink[]
  backlinks: NotebookBacklink[]
  collapsed: boolean
  onCollapsedChange: (collapsed: boolean) => void
  onSelectEntry: (id: string) => void
  onCreateLinkedEntry: (title: string) => void
}) {
  const outgoing = links.slice(0, 5)
  const incoming = backlinks.slice(0, 5)
  const hasNodes = outgoing.length > 0 || incoming.length > 0

  return (
    <section>
      <button
        className="mb-2 flex w-full items-center gap-2 rounded-lg px-1 py-1 text-left text-xs font-semibold uppercase tracking-wide text-gray-500 hover:bg-gray-50 hover:text-blue-700"
        type="button"
        aria-expanded={!collapsed}
        onClick={() => onCollapsedChange(!collapsed)}
      >
        <Network size={14} />
        <span className="flex-1">Local graph</span>
        <ChevronRight size={14} className={collapsed ? '' : 'rotate-90'} />
      </button>
      {!collapsed && (
      <div className="rounded-xl border border-blue-50 bg-gradient-to-b from-blue-50/70 to-white p-3">
        <div className="flex justify-center">
          <div className="max-w-full truncate rounded-full bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white shadow-sm">
            {entry.title}
          </div>
        </div>
        {!hasNodes ? (
          <div className="mt-3 text-center text-xs text-gray-400">No linked notes yet</div>
        ) : (
          <div className="mt-4 grid grid-cols-2 gap-2">
            <div className="space-y-2">
              <div className="text-[11px] font-semibold uppercase tracking-wide text-blue-700">Out</div>
              {outgoing.length === 0 ? (
                <div className="text-xs text-gray-400">None</div>
              ) : (
                outgoing.map((link) => (
                  <button
                    key={`graph-out-${link.raw}-${link.target_id ?? link.target}`}
                    className={`w-full truncate rounded-lg border px-2 py-1.5 text-left text-xs transition ${
                      link.resolved
                        ? 'border-blue-100 bg-white text-blue-800 hover:bg-blue-50'
                        : 'border-dashed border-amber-200 bg-amber-50 text-amber-800 hover:bg-amber-100'
                    }`}
                    type="button"
                    onClick={() => {
                      if (link.target_id) onSelectEntry(link.target_id)
                      else onCreateLinkedEntry(link.target)
                    }}
                  >
                    {link.alias || link.target_title || link.target}
                  </button>
                ))
              )}
            </div>
            <div className="space-y-2">
              <div className="text-[11px] font-semibold uppercase tracking-wide text-blue-700">In</div>
              {incoming.length === 0 ? (
                <div className="text-xs text-gray-400">None</div>
              ) : (
                incoming.map((backlink) => (
                  <button
                    key={`graph-in-${backlink.source_entry_id}-${backlink.raw}`}
                    className="w-full truncate rounded-lg border border-gray-100 bg-white px-2 py-1.5 text-left text-xs text-gray-700 transition hover:border-blue-100 hover:bg-blue-50"
                    type="button"
                    onClick={() => onSelectEntry(backlink.source_entry_id)}
                  >
                    {backlink.source_title}
                  </button>
                ))
              )}
            </div>
          </div>
        )}
      </div>
      )}
    </section>
  )
}

function QuizBankTab({
  quizzes,
  filteredQuizzes,
  sourceFilter,
  activeQuiz,
  activeQuestion,
  questionIndex,
  status,
  onSelectQuiz,
  onSourceFilterChange,
  onDeleteQuiz,
  onQuestionIndexChange,
  onSourceNavigate,
  onStartQuiz,
  language,
}: {
  quizzes: QuizSession[]
  filteredQuizzes: QuizSession[]
  sourceFilter: QuizSourceFilter
  activeQuiz: QuizSession | null
  activeQuestion: QuizQuestion | null
  questionIndex: number
  status: string
  onSelectQuiz: (id: string) => void
  onSourceFilterChange: (filter: QuizSourceFilter) => void
  onDeleteQuiz: (quiz: QuizSession) => void
  onQuestionIndexChange: (index: number) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  onStartQuiz?: () => void
  language: 'zh-CN' | 'en-US'
}) {
  const activeAnswer = activeQuiz && activeQuestion
    ? activeQuiz.answers.find((answer) => answer.question_id === activeQuestion.id) ?? null
    : null
  const missedQuestions = useMemo(() => {
    if (!activeQuiz) return []
    return activeQuiz.questions.filter((question) => {
      const answer = activeQuiz.answers.find((item) => item.question_id === question.id)
      return answer && !answer.correct
    })
  }, [activeQuiz])

  return (
    <div className="flex min-h-0 flex-1">
      <aside className="flex w-80 shrink-0 flex-col border-r border-gray-100 bg-gray-50/70">
        <div className="px-4 py-3 text-xs text-gray-500">{status}</div>
        <div className="border-b border-gray-100 px-3 pb-3">
          <div className="flex flex-wrap gap-2">
            {quizSourceFilters.map((filter) => (
              <button
                key={filter.key}
                className={`rounded-full px-3 py-1.5 text-xs font-medium transition ${
                  sourceFilter === filter.key
                    ? 'bg-blue-600 text-white'
                    : 'bg-white text-gray-600 ring-1 ring-gray-200 hover:bg-blue-50 hover:text-blue-700'
                }`}
                type="button"
                onClick={() => onSourceFilterChange(filter.key)}
              >
                {filter.label}
              </button>
            ))}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-3 pb-4">
          {quizzes.length === 0 ? (
            <div className="rounded-lg px-3 py-8 text-center text-sm text-gray-400">No quiz records yet</div>
          ) : filteredQuizzes.length === 0 ? (
            <div className="rounded-lg px-3 py-8 text-center text-sm text-gray-400">No quizzes match this filter</div>
          ) : (
            <div className="space-y-2">
              {filteredQuizzes.map((quiz) => {
                const active = activeQuiz?.id === quiz.id
                const score = quiz.score ?? { correct: 0, total: quiz.questions.length }
                const answeredCount = quiz.answers.length
                const source = quizSourceType(quiz)
                return (
                  <button
                    key={quiz.id}
                    className={`group flex w-full items-start gap-3 rounded-lg p-3 text-left text-sm ${
                      active ? 'bg-white shadow-sm ring-1 ring-blue-100' : 'hover:bg-white'
                    }`}
                    type="button"
                    onClick={() => onSelectQuiz(quiz.id)}
                  >
                    <FileQuestion size={17} className="mt-0.5 shrink-0 text-blue-600" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium text-gray-900">{quiz.title}</span>
                      <span className="mt-0.5 block text-xs text-gray-500">
                        {score.correct}/{score.total} correct · answered {answeredCount}/{quiz.questions.length} · {quiz.status}
                      </span>
                      <span className="mt-1 flex items-center gap-2 text-xs text-gray-400">
                        <span className="rounded-full bg-gray-100 px-2 py-0.5 text-gray-600">{quizSourceLabel(source)}</span>
                        Created {formatTime(quiz.created_at)} · Updated {formatTime(quiz.updated_at)}
                      </span>
                    </span>
                    <span
                      role="button"
                      tabIndex={0}
                      className="rounded p-1 text-gray-400 opacity-0 hover:bg-red-50 hover:text-red-600 group-hover:opacity-100"
                      onClick={(event) => {
                        event.stopPropagation()
                        onDeleteQuiz(quiz)
                      }}
                    >
                      <Trash2 size={15} />
                    </span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        {!activeQuiz || !activeQuestion ? (
          <EmptyDetail
            icon={FileQuestion}
            title={language === 'en-US' ? 'Select a quiz record' : '选择一条测验记录'}
            description={language === 'en-US'
              ? 'Quiz Bank is for review and re-practice. Generate new quizzes from the chat composer.'
              : '题库用于回顾和再次练习。新的测验从聊天输入框中生成。'}
            actionLabel={quizzes.length === 0
              ? language === 'en-US' ? 'Generate a quiz' : '生成测验'
              : undefined}
            onAction={quizzes.length === 0 ? onStartQuiz : undefined}
          />
        ) : (
          <>
            <div className="flex items-center border-b border-gray-100 px-8 py-5">
              <div>
                <h3 className="text-xl font-semibold text-gray-950">{activeQuiz.title}</h3>
                <p className="mt-1 text-sm text-gray-500">
                  Question {questionIndex + 1} of {activeQuiz.questions.length} · {difficultyLabel(activeQuestion.difficulty)}
                </p>
              </div>
              <ScorePill quiz={activeQuiz} />
            </div>

            <div className="flex-1 overflow-y-auto px-8 py-6">
              <div className="max-w-4xl">
                <div className="mb-4 flex flex-wrap gap-2">
                  {activeQuestion.tags.map((tag) => (
                    <span key={tag} className="rounded-full bg-gray-100 px-2.5 py-1 text-xs font-medium text-gray-600">{tag}</span>
                  ))}
                </div>
                <h4 className="text-2xl font-semibold leading-9 text-gray-950">{activeQuestion.stem}</h4>

                <div className="mt-6 space-y-3">
                  {activeQuestion.options.map((option) => {
                    const selected = activeAnswer?.selected_option_id === option.id
                    const correct = activeQuestion.correct_option_id === option.id
                    return (
                      <div
                        key={option.id}
                        className={`flex items-start gap-3 rounded-lg border p-4 ${
                          correct
                            ? 'border-emerald-300 bg-emerald-50'
                            : selected
                              ? 'border-red-200 bg-red-50'
                              : 'border-gray-200 bg-white'
                        }`}
                      >
                        <span className={correct ? 'text-emerald-700' : selected ? 'text-red-600' : 'text-gray-400'}>
                          <CheckCircle2 size={19} />
                        </span>
                        <span>
                          <span className="font-medium text-gray-950">{option.id}.</span>{' '}
                          <span className="text-gray-700">{option.text}</span>
                        </span>
                      </div>
                    )
                  })}
                </div>

                <section className="mt-6 rounded-lg border border-gray-200 bg-gray-50 p-4">
                  <div className="text-sm font-semibold text-gray-950">Explanation</div>
                  <p className="mt-2 text-sm leading-6 text-gray-600">{activeQuestion.explanation}</p>
                  {activeQuestion.citations.length > 0 && (
                    <QuizSourceReferences
                      quizId={activeQuiz.id}
                      questionId={activeQuestion.id}
                      citations={activeQuestion.citations}
                      onSourceNavigate={onSourceNavigate}
                    />
                  )}
                </section>

                {missedQuestions.length > 0 && (
                  <section className="mt-6 rounded-lg border border-blue-100 bg-blue-50/50 p-4">
                    <div className="text-sm font-semibold text-gray-950">Missed-question review</div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      {missedQuestions.map((question) => {
                        const index = activeQuiz.questions.findIndex((item) => item.id === question.id)
                        return (
                          <button
                            key={question.id}
                            className="rounded-full bg-white px-3 py-1.5 text-xs font-medium text-blue-700 ring-1 ring-blue-100 hover:bg-blue-50"
                            type="button"
                            onClick={() => {
                              if (index >= 0) onQuestionIndexChange(index)
                            }}
                          >
                            Q{index + 1}
                          </button>
                        )
                      })}
                    </div>
                  </section>
                )}
              </div>
            </div>

            <footer className="flex items-center gap-3 border-t border-gray-100 px-8 py-4">
              <button className={secondaryButtonClassName} type="button" disabled={questionIndex === 0} onClick={() => onQuestionIndexChange(Math.max(0, questionIndex - 1))}>
                <ChevronLeft size={16} />
                Previous
              </button>
              <button className={secondaryButtonClassName} type="button" disabled={questionIndex >= activeQuiz.questions.length - 1} onClick={() => onQuestionIndexChange(Math.min(activeQuiz.questions.length - 1, questionIndex + 1))}>
                Next
                <ChevronRight size={16} />
              </button>
            </footer>
          </>
        )}
      </div>
    </div>
  )
}

function QuizSourceReferences({
  quizId,
  questionId,
  citations,
  onSourceNavigate,
}: {
  quizId: string
  questionId: string
  citations: QuizQuestion['citations']
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  return (
    <SourceReferences
      id={`space-quiz-citations-${quizId}-${questionId}`}
      references={citations.map((citation, index) => quizCitationToSourceReference(citation, index))}
      onNavigate={onSourceNavigate}
    />
  )
}

function quizCitationToSourceReference(citation: QuizQuestion['citations'][number], index: number): SourceReference {
  const raw = quizCitationRawTarget(citation)
  const target = sourceTargetFromRaw(raw)
  return {
    id: `${index + 1}:${raw}`,
    label: String(index + 1),
    raw,
    surface: target?.type === 'web' ? 'web' : target?.type === 'kb' ? 'kb' : 'unknown',
    title: citation.title || citation.source,
    description: citation.text,
    score: citation.score,
    metadata: {
      documentName: citation.title || citation.source,
      documentId: citation.document_id ?? undefined,
      chunkId: citation.chunk_id ?? undefined,
      missingReason: target ? undefined : 'This quiz citation was generated before source navigation metadata was available.',
    },
    target,
  }
}

function quizCitationRawTarget(citation: QuizQuestion['citations'][number]) {
  if (citation.kb && citation.document_id) {
    return ['kb', citation.kb, citation.document_id, citation.chunk_id].filter(Boolean).join(':')
  }
  return citation.source
}

function StudentProfileTab({
  profile,
  memoryFiles,
  editingMemoryPath,
  memoryDraft,
  loading,
  onStartEditMemory,
  onCancelEditMemory,
  onMemoryDraftChange,
  onSaveMemoryFile,
  onSourceNavigate,
}: {
  profile: ReturnType<typeof buildProfile>
  memoryFiles: MemoryFile[]
  editingMemoryPath: string | null
  memoryDraft: string
  loading: boolean
  onStartEditMemory: (file: MemoryFile) => void
  onCancelEditMemory: () => void
  onMemoryDraftChange: (value: string) => void
  onSaveMemoryFile: (path: string) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const filesByPath = new Map(memoryFiles.map((file) => [file.path, file]))

  return (
    <div className="flex-1 overflow-y-auto px-8 py-6">
      <div className="grid gap-4 md:grid-cols-3">
        <Metric label="Quiz records" value={String(profile.quizCount)} />
        <Metric label="Answered" value={String(profile.answeredCount)} />
        <Metric label="Accuracy" value={profile.accuracyLabel} />
      </div>

      <div className="mt-6 grid gap-5 lg:grid-cols-2">
        <section className="rounded-lg border border-gray-200 bg-white p-5">
          <div className="flex items-center gap-2 text-sm font-semibold text-gray-950">
            <Target size={18} className="text-blue-600" />
            Current weak signals
          </div>
          {profile.weakTags.length === 0 ? (
            <p className="mt-3 text-sm leading-6 text-gray-500">No wrong-answer pattern yet. Finish quizzes to build this profile.</p>
          ) : (
            <div className="mt-4 flex flex-wrap gap-2">
              {profile.weakTags.map(([tag, count]) => (
                <span key={tag} className="rounded-full bg-blue-50 px-3 py-1.5 text-sm font-medium text-blue-700">
                  {tag} x {count}
                </span>
              ))}
            </div>
          )}
        </section>

        <section className="rounded-lg border border-gray-200 bg-white p-5">
          <div className="text-sm font-semibold text-gray-950">Profile source</div>
          <p className="mt-3 text-sm leading-6 text-gray-500">
            Student Profile is rendered from visible Markdown memory plus quiz stats. Edit the Markdown below to correct the profile.
          </p>
        </section>
      </div>

      <div className="mt-6 grid gap-5 xl:grid-cols-3">
        {profileMemoryPaths.map((path) => {
          const file = filesByPath.get(path)
          return (
            <MemoryProfileCard
              key={path}
              path={path}
              file={file}
              editing={editingMemoryPath === path}
              draft={memoryDraft}
              loading={loading}
              onStartEdit={onStartEditMemory}
              onCancel={onCancelEditMemory}
              onDraftChange={onMemoryDraftChange}
              onSave={onSaveMemoryFile}
              onSourceNavigate={onSourceNavigate}
            />
          )
        })}
      </div>
    </div>
  )
}

function MemoryProfileCard({
  path,
  file,
  editing,
  draft,
  loading,
  onStartEdit,
  onCancel,
  onDraftChange,
  onSave,
  onSourceNavigate,
}: {
  path: string
  file?: MemoryFile
  editing: boolean
  draft: string
  loading: boolean
  onStartEdit: (file: MemoryFile) => void
  onCancel: () => void
  onDraftChange: (value: string) => void
  onSave: (path: string) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const title = memoryFileLabel(path)
  return (
    <section className="flex min-h-[360px] flex-col rounded-lg border border-gray-200 bg-white">
      <div className="flex items-center gap-3 border-b border-gray-100 px-4 py-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-gray-950">{title}</div>
          <div className="truncate text-xs text-gray-500">{path}</div>
        </div>
        {file && !editing && (
          <button className={`${secondaryButtonClassName} ml-auto`} type="button" disabled={loading} onClick={() => onStartEdit(file)}>
            <Edit3 size={16} />
            Edit
          </button>
        )}
        {editing && (
          <div className="ml-auto flex items-center gap-2">
            <button className={secondaryButtonClassName} type="button" disabled={loading || !draft.trim()} onClick={() => onSave(path)}>
              <Save size={16} />
              Save
            </button>
            <button className={secondaryButtonClassName} type="button" disabled={loading} onClick={onCancel}>
              <X size={16} />
              Cancel
            </button>
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {!file ? (
          <div className="text-sm text-gray-400">Memory file has not loaded yet.</div>
        ) : editing ? (
          <textarea
            className={`${inputClassName} min-h-[260px] resize-y font-mono leading-6`}
            value={draft}
            onChange={(event) => onDraftChange(event.target.value)}
          />
        ) : (
          <div className="space-y-4">
            <div className="prose-sm max-w-none">
              <MarkdownMessage text={file.markdown || ' '} onSourceNavigate={onSourceNavigate} />
            </div>
          </div>
        )}
      </div>
    </section>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-gray-200 bg-white p-5">
      <div className="text-xs font-medium uppercase tracking-wide text-gray-500">{label}</div>
      <div className="mt-2 text-2xl font-semibold text-gray-950">{value}</div>
    </div>
  )
}

function ScorePill({ quiz }: { quiz: QuizSession }) {
  const score = quiz.score ?? { correct: 0, total: quiz.questions.length }
  return (
    <div className="ml-auto rounded-lg bg-blue-50 px-3 py-2 text-sm font-medium text-blue-700">
      Score {score.correct}/{score.total}
    </div>
  )
}

function EmptyDetail({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
}: {
  icon: typeof FileQuestion
  title: string
  description: string
  actionLabel?: string
  onAction?: () => void
}) {
  return (
    <div className="flex flex-1 items-center justify-center px-6">
      <div className="max-w-md text-center">
        <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50 text-blue-700">
          <Icon size={28} />
        </div>
        <h3 className="mt-5 text-2xl font-semibold text-gray-950">{title}</h3>
        <p className="mt-2 text-sm leading-6 text-gray-500">{description}</p>
        {actionLabel && onAction && (
          <button
            type="button"
            className="mt-5 inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-4 text-sm font-medium text-white hover:bg-blue-700"
            onClick={onAction}
          >
            <Plus size={16} />
            {actionLabel}
          </button>
        )}
      </div>
    </div>
  )
}

function buildProfile(quizzes: QuizSession[]) {
  let answeredCount = 0
  let correctCount = 0
  const weakTags = new Map<string, number>()

  quizzes.forEach((quiz) => {
    quiz.answers.forEach((answer) => {
      answeredCount += 1
      if (answer.correct) correctCount += 1
      if (!answer.correct) {
        const question = quiz.questions.find((item) => item.id === answer.question_id)
        question?.tags.forEach((tag) => weakTags.set(tag, (weakTags.get(tag) ?? 0) + 1))
      }
    })
  })

  const accuracy = answeredCount ? Math.round((correctCount / answeredCount) * 100) : null
  return {
    quizCount: quizzes.length,
    answeredCount,
    accuracyLabel: accuracy == null ? 'No data' : `${accuracy}%`,
    weakTags: [...weakTags.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8),
  }
}

function memoryFileLabel(path: string) {
  const labels: Record<string, string> = {
    'L3/profile.md': 'Student profile',
    'L3/recent.md': 'Recent context',
    'L3/teaching_strategy.md': 'Teaching strategy',
  }
  return labels[path] ?? path
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

  for (const folderPath of folderPaths) {
    ensureFolder(notebookPathSegments(folderPath))
  }

  for (const entry of entries) {
    const segments = notebookEntryPathSegments(entry)
    const fileName = segments.pop() ?? `${entry.title || 'Untitled note'}.md`
    const children = ensureFolder(segments)
    children.push({
      type: 'entry',
      name: fileName,
      path: [...segments, fileName].join('/'),
      entry,
    })
    sortNotebookTreeNodes(children)
  }

  sortNotebookTreeNodes(root)
  return root
}

function collectNotebookFolderPaths(nodes: NotebookTreeNode[]) {
  const paths: string[] = []
  const visit = (items: NotebookTreeNode[]) => {
    for (const item of items) {
      if (item.type === 'folder') {
        paths.push(item.path)
        visit(item.children)
      }
    }
  }
  visit(nodes)
  return paths
}

function flattenVisibleNotebookTree(
  nodes: NotebookTreeNode[],
  expandedFolders: Set<string>,
  limit = Number.POSITIVE_INFINITY,
) {
  const rows: FlatNotebookTreeRow[] = []
  const visit = (items: NotebookTreeNode[], depth: number) => {
    for (const item of items) {
      if (rows.length >= limit) return
      rows.push({ node: item, depth })
      if (item.type === 'folder' && expandedFolders.has(item.path)) {
        visit(item.children, depth + 1)
      }
    }
  }
  visit(nodes, 0)
  return rows
}

function loadExpandedNotebookFolders() {
  try {
    const raw = window.localStorage.getItem(NOTEBOOK_EXPANDED_FOLDERS_KEY)
    if (!raw) return new Set<string>()
    const values = JSON.parse(raw)
    if (!Array.isArray(values)) return new Set<string>()
    return new Set(values.filter((value): value is string => typeof value === 'string' && value.trim().length > 0))
  } catch {
    return new Set<string>()
  }
}

function saveExpandedNotebookFolders(folders: Set<string>) {
  try {
    window.localStorage.setItem(NOTEBOOK_EXPANDED_FOLDERS_KEY, JSON.stringify([...folders].sort()))
  } catch {
    // localStorage can be unavailable in restricted browser contexts.
  }
}

function setsEqual(left: Set<string>, right: Set<string>) {
  if (left.size !== right.size) return false
  for (const item of left) {
    if (!right.has(item)) return false
  }
  return true
}

function notebookEntryPathSegments(entry: NotebookEntry) {
  const rawPath = (entry.path || `${entry.title || 'Untitled note'}.md`).replace(/\\/g, '/')
  return notebookPathSegments(rawPath, `${entry.title || 'Untitled note'}.md`)
}

function notebookPathSegments(path: string, fallback = '') {
  const rawPath = (path || fallback).replace(/\\/g, '/')
  const segments = rawPath
    .split('/')
    .map((segment) => segment.trim())
    .filter((segment) => segment && segment !== '.' && segment !== '..')
  if (segments.length === 0) return fallback ? [fallback] : []
  return segments
}

function sortNotebookTreeNodes(nodes: NotebookTreeNode[]) {
  nodes.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'folder' ? -1 : 1
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true })
  })
}

function subtitleFor(tab: SpaceTab, t: (key: TranslationKey) => string) {
  if (tab === 'notebook') return t('space.tabs.notebook.description')
  if (tab === 'quiz_bank') return t('space.tabs.quizBank.description')
  return t('space.tabs.studentProfile.description')
}

async function safeJson(res: Response): Promise<Record<string, unknown>> {
  try {
    return await res.json()
  } catch {
    return {}
  }
}

function errorMessage(data: Record<string, unknown>, status: number) {
  return typeof data.error === 'string' ? data.error : `HTTP ${status}`
}

function difficultyLabel(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1)
}

function formatTime(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

const secondaryButtonClassName = 'inline-flex h-9 items-center justify-center gap-1.5 rounded-lg border border-gray-200 px-3.5 text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50'
const inputClassName = 'w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-300 focus:ring-2 focus:ring-blue-50'
