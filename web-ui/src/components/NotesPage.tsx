import { useCallback, useEffect, useMemo, useState } from 'react'
import { Edit3, FileText, Plus, RefreshCw, Save, Trash2, Undo2, X } from 'lucide-react'
import { MarkdownMessage } from './MarkdownMessage'
import type { SourceReference, SourceTarget } from './MarkdownMessage'

interface NoteEntry {
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
  revision?: string
}

interface Props {
  language: 'zh-CN' | 'en-US'
  focusTarget?: Extract<SourceTarget, { type: 'notebook' }> | null
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}

export function NotesPage({ language, focusTarget, onSourceNavigate }: Props) {
  const [notes, setNotes] = useState<NoteEntry[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [detail, setDetail] = useState<NoteEntry | null>(null)
  const [editing, setEditing] = useState(false)
  const [title, setTitle] = useState('')
  const [path, setPath] = useState('')
  const [markdown, setMarkdown] = useState('')
  const [deleted, setDeleted] = useState<NoteEntry | null>(null)
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('')

  const active = detail?.id === activeId ? detail : notes.find((note) => note.id === activeId) ?? null
  const sorted = useMemo(() => [...notes].sort((left, right) => (left.path || left.title).localeCompare(right.path || right.title)), [notes])

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/entries?space_id=default')
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const next = (data.entries ?? []) as NoteEntry[]
      setNotes(next)
      setActiveId((current) => current && next.some((note) => note.id === current) ? current : next[0]?.id ?? null)
      setStatus('')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [])

  const loadDetail = useCallback(async (id: string) => {
    try {
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(id)}`)
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const entry = { ...(data.entry as NoteEntry), revision: data.revision as string }
      setDetail(entry)
      setNotes((items) => items.map((item) => item.id === id ? { ...item, ...entry } : item))
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    }
  }, [])

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (focusTarget?.entryId) setActiveId(focusTarget.entryId)
  }, [focusTarget?.entryId])
  useEffect(() => {
    if (!activeId) { setDetail(null); return }
    if (detail?.id !== activeId) void loadDetail(activeId)
  }, [activeId, detail?.id, loadDetail])

  const startEdit = (note: NoteEntry) => {
    setTitle(note.title)
    setPath(note.path ?? '')
    setMarkdown(note.markdown ?? '')
    setEditing(true)
  }

  const create = async () => {
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ space_id: 'default', entry_type: 'note', title: 'Untitled note', markdown: '# Untitled note\n\n' }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const note = { ...(data.entry as NoteEntry), revision: data.revision as string }
      setNotes((items) => [note, ...items])
      setActiveId(note.id)
      setDetail(note)
      startEdit(note)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }

  const save = async () => {
    if (!active || !markdown.trim()) return
    setLoading(true)
    try {
      let revision = active.revision
      if (!revision) {
        const response = await fetch(`/api/notebook/entries/${encodeURIComponent(active.id)}`)
        const data = await safeJson(response)
        if (!response.ok) throw new Error(errorMessage(data, response.status))
        revision = data.revision as string
      }
      const nextTitle = title.trim() || 'Untitled note'
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(active.id)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expected_revision: revision, title: nextTitle, path: path.trim() || `${nextTitle}.md`, markdown }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const updated = { ...(data.entry as NoteEntry), revision: data.revision as string }
      setNotes((items) => items.map((item) => item.id === updated.id ? updated : item))
      setDetail(updated)
      setEditing(false)
      setStatus(language === 'en-US' ? 'Note saved' : '笔记已保存')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }

  const remove = async (note: NoteEntry) => {
    if (!window.confirm(language === 'en-US' ? `Delete "${note.title}"?` : `确定删除“${note.title}”吗？`)) return
    try {
      let restorable = detail?.id === note.id ? detail : note
      if (!restorable.markdown) {
        const response = await fetch(`/api/notebook/entries/${encodeURIComponent(note.id)}`)
        const data = await safeJson(response)
        if (!response.ok) throw new Error(errorMessage(data, response.status))
        restorable = data.entry as NoteEntry
      }
      const response = await fetch(`/api/notebook/entries/${encodeURIComponent(note.id)}`, { method: 'DELETE' })
      if (!response.ok) throw new Error(errorMessage(await safeJson(response), response.status))
      setDeleted(restorable)
      setNotes((items) => items.filter((item) => item.id !== note.id))
      setActiveId((current) => current === note.id ? null : current)
      setDetail(null)
      setEditing(false)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    }
  }

  const restore = async () => {
    if (!deleted) return
    setLoading(true)
    try {
      const response = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: deleted.space_id,
          entry_type: deleted.entry_type,
          title: deleted.title,
          path: deleted.path,
          markdown: deleted.markdown,
          metadata: deleted.metadata,
          source_session_id: deleted.source_session_id,
          source_message_id: deleted.source_message_id,
        }),
      })
      const data = await safeJson(response)
      if (!response.ok) throw new Error(errorMessage(data, response.status))
      const restored = { ...(data.entry as NoteEntry), revision: data.revision as string }
      setNotes((items) => [restored, ...items])
      setActiveId(restored.id)
      setDetail(restored)
      setDeleted(null)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }

  return <main className="flex h-full min-h-0 flex-col bg-white">
    <header className="flex items-start gap-3 border-b border-gray-200 px-6 py-4">
      <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700"><FileText size={21} /></span>
      <div>
        <h1 className="text-xl font-semibold text-gray-950">{language === 'en-US' ? 'Notebook' : '笔记'}</h1>
        <p className="mt-1 text-sm text-gray-500">{language === 'en-US' ? 'Record, organize, read, and edit your Markdown notes.' : '记录、整理、查看和编辑你拥有的 Markdown 笔记。'}</p>
      </div>
    </header>
    <div className="flex min-h-0 flex-1">
    <aside className="flex w-80 shrink-0 flex-col border-r border-gray-200 bg-gray-50/70">
      <div className="flex items-center gap-2 border-b border-gray-200 p-4">
        <button className="inline-flex h-9 flex-1 items-center justify-center gap-2 rounded-lg bg-blue-600 text-sm font-medium text-white" type="button" disabled={loading} onClick={() => void create()}><Plus size={16} />{language === 'en-US' ? 'New note' : '新建笔记'}</button>
        <button className="rounded-lg border border-gray-200 bg-white p-2 text-gray-600" type="button" disabled={loading} onClick={() => void load()} aria-label="Refresh notes"><RefreshCw size={16} className={loading ? 'animate-spin' : ''} /></button>
      </div>
      {deleted && <div className="flex items-center gap-2 border-b border-amber-100 bg-amber-50 px-3 py-2 text-xs text-amber-900"><span className="min-w-0 flex-1 truncate">{language === 'en-US' ? `Deleted “${deleted.title}”` : `已删除“${deleted.title}”`}</span><button type="button" className="inline-flex items-center gap-1 font-medium" onClick={() => void restore()}><Undo2 size={13} />{language === 'en-US' ? 'Undo' : '撤销'}</button><button type="button" onClick={() => setDeleted(null)}><X size={13} /></button></div>}
      <div className="min-h-0 flex-1 overflow-y-auto p-3">{sorted.map((note) => <button key={note.id} type="button" className={`mb-1 flex w-full items-start gap-2 rounded-lg px-3 py-2 text-left ${note.id === activeId ? 'bg-white text-blue-700 shadow-sm' : 'text-gray-700 hover:bg-white'}`} onClick={() => { setActiveId(note.id); setEditing(false) }}><FileText size={16} className="mt-0.5 shrink-0" /><span className="min-w-0"><span className="block truncate text-sm font-medium">{note.title}</span><span className="block truncate text-xs text-gray-400">{note.path || 'Unfiled'}</span></span></button>)}</div>
      {status && <div className="border-t border-gray-200 px-4 py-2 text-xs text-gray-500">{status}</div>}
    </aside>
    <section className="flex min-w-0 flex-1 flex-col">{!active ? <div className="m-auto text-center text-gray-400"><FileText className="mx-auto" size={34} /><p className="mt-3 text-sm">{language === 'en-US' ? 'Create or select a note' : '新建或选择一条笔记'}</p></div> : <>
      <header className="flex items-start gap-4 border-b border-gray-100 px-7 py-4"><div className="min-w-0 flex-1">{editing ? <div className="grid max-w-2xl gap-2"><input className={inputClassName} value={title} onChange={(event) => setTitle(event.target.value)} aria-label="Note title" /><input className={`${inputClassName} font-mono text-xs`} value={path} onChange={(event) => setPath(event.target.value)} aria-label="Note path" placeholder="folder/note.md" /></div> : <><h2 className="truncate text-xl font-semibold text-gray-950">{active.title}</h2><p className="mt-1 truncate text-xs text-gray-400">{active.path}</p></>}</div><div className="flex gap-2">{editing ? <><button className={buttonClassName} type="button" disabled={loading || !markdown.trim()} onClick={() => void save()}><Save size={15} />{language === 'en-US' ? 'Save' : '保存'}</button><button className={buttonClassName} type="button" onClick={() => setEditing(false)}><X size={15} />{language === 'en-US' ? 'Cancel' : '取消'}</button></> : <><button className={buttonClassName} type="button" onClick={() => startEdit(active)}><Edit3 size={15} />{language === 'en-US' ? 'Edit' : '编辑'}</button><button className={buttonClassName} type="button" onClick={() => void remove(active)}><Trash2 size={15} />{language === 'en-US' ? 'Delete' : '删除'}</button></>}</div></header>
      <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">{editing ? <textarea className={`${inputClassName} min-h-[65vh] resize-y font-mono leading-6`} value={markdown} onChange={(event) => setMarkdown(event.target.value)} /> : <div className="max-w-4xl rounded-lg border border-gray-200 bg-gray-50 p-5"><MarkdownMessage text={active.markdown || ' '} onSourceNavigate={onSourceNavigate} /></div>}</div>
    </>}</section>
    </div>
  </main>
}

const inputClassName = 'w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-100'
const buttonClassName = 'inline-flex items-center gap-2 rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50'

async function safeJson(response: Response): Promise<Record<string, unknown>> {
  try { return await response.json() as Record<string, unknown> } catch { return {} }
}

function errorMessage(data: Record<string, unknown>, status: number) {
  return typeof data.error === 'string' ? data.error : `HTTP ${status}`
}
