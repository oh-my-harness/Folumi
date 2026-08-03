import { useCallback, useEffect, useState } from 'react'
import { Brain, Edit3, ExternalLink, RefreshCw, Save, Trash2, X } from 'lucide-react'
import type { SourceReference, SourceTarget } from './MarkdownMessage'
import { sourceTargetFromRaw } from './MarkdownMessage'

interface MemoryItem {
  path: string
  level: 'L2' | 'L3'
  file_revision: string
  marker: string
  revision: string
  section?: string | null
  text: string
  source_refs: string[]
  provenance?: Record<string, unknown> | null
  kind?: string | null
  expires_at?: string | null
}

interface Props {
  language: 'zh-CN' | 'en-US'
  enabled: boolean
  onEnabledChange: (enabled: boolean) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}

export function UserMemoryPage({ language, enabled, onEnabledChange, onSourceNavigate }: Props) {
  const [items, setItems] = useState<MemoryItem[]>([])
  const [loading, setLoading] = useState(true)
  const [status, setStatus] = useState('')
  const [editing, setEditing] = useState<MemoryItem | null>(null)
  const [draft, setDraft] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const response = await fetch('/api/memory/items')
      const data = await response.json().catch(() => ({})) as { items?: MemoryItem[]; error?: string }
      if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`)
      setItems(data.items ?? [])
      setStatus('')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void load() }, [load])

  const save = async () => {
    if (!editing || !draft.trim()) return
    const response = await fetch('/api/memory/items', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: editing.path, marker: editing.marker, file_revision: editing.file_revision, text: draft }),
    })
    const data = await response.json().catch(() => ({})) as { error?: string }
    if (!response.ok) {
      setStatus(data.error || `HTTP ${response.status}`)
      return
    }
    setEditing(null)
    setDraft('')
    await load()
  }

  const forget = async (item: MemoryItem) => {
    const confirmed = window.confirm(language === 'en-US'
      ? `Forget this memory?\n\n${item.text}`
      : `确定遗忘这条记忆吗？\n\n${item.text}`)
    if (!confirmed) return
    const response = await fetch('/api/memory/items', {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: item.path, marker: item.marker, file_revision: item.file_revision }),
    })
    const data = await response.json().catch(() => ({})) as { error?: string }
    if (!response.ok) {
      setStatus(data.error || `HTTP ${response.status}`)
      return
    }
    await load()
  }

  const openSource = (raw: string) => {
    const target = sourceTargetFromRaw(raw)
    if (!target || !onSourceNavigate) return
    onSourceNavigate(target, { id: raw, label: raw, raw, surface: target.type, target })
  }

  return (
    <section className="h-full overflow-y-auto bg-white px-6 py-6">
      <div className="flex items-start gap-3 border-b border-gray-200 pb-5">
        <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700"><Brain size={21} /></span>
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-semibold text-gray-950">{language === 'en-US' ? 'Memory' : '记忆'}</h1>
          <p className="mt-1 text-sm text-gray-500">{language === 'en-US' ? 'Review and control the long-term context the assistant may carry across conversations.' : '查看和控制助手可以跨会话延续的长期信息。'}</p>
        </div>
        <label className="flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm text-gray-700">
          <input type="checkbox" checked={enabled} onChange={(event) => onEnabledChange(event.target.checked)} />
          {language === 'en-US' ? 'Use memory' : '启用记忆'}
        </label>
        <button type="button" className="inline-flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm text-gray-700" disabled={loading} onClick={() => void load()}><RefreshCw size={15} className={loading ? 'animate-spin' : ''} />{language === 'en-US' ? 'Refresh' : '刷新'}</button>
      </div>

      <div className={`mt-5 rounded-lg px-4 py-3 text-sm ${enabled ? 'bg-emerald-50 text-emerald-800' : 'bg-gray-100 text-gray-600'}`}>
        {enabled
          ? (language === 'en-US' ? 'Memory is on for new sessions.' : '新会话已启用长期记忆。')
          : (language === 'en-US' ? 'Memory is off. Existing items remain here until you edit or forget them.' : '记忆已关闭；已有内容会保留，直到你编辑或遗忘。')}
      </div>
      {status && <div className="mt-4 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{status}</div>}

      <div className="mt-5 space-y-3">
        {!loading && items.length === 0 && <div className="rounded-lg border border-dashed border-gray-200 px-5 py-10 text-center text-sm text-gray-400">{language === 'en-US' ? 'No long-term memory yet.' : '还没有长期记忆。'}</div>}
        {items.map((item) => {
          const isEditing = editing?.marker === item.marker && editing.path === item.path
          return (
            <article key={`${item.path}:${item.marker}`} className="rounded-lg border border-gray-200 p-4">
              <div className="flex items-start gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-gray-500"><span className="rounded bg-gray-100 px-2 py-0.5">{item.level === 'L3' ? (language === 'en-US' ? 'Long-term' : '长期') : (language === 'en-US' ? 'Summary' : '摘要')}</span><span>{item.section || item.kind || item.path}</span>{item.expires_at && <span>· {language === 'en-US' ? 'expires' : '到期'} {new Date(item.expires_at).toLocaleString()}</span>}</div>
                  {isEditing ? <textarea className="mt-3 min-h-24 w-full resize-y rounded-lg border border-blue-200 px-3 py-2 text-sm leading-6 outline-none" value={draft} onChange={(event) => setDraft(event.target.value)} /> : <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-gray-800">{item.text}</p>}
                  <div className="mt-3 flex flex-wrap gap-2">
                    {item.source_refs.map((source) => <button key={source} type="button" className="inline-flex items-center gap-1 rounded-full bg-blue-50 px-2.5 py-1 text-xs text-blue-700" onClick={() => openSource(source)}><ExternalLink size={12} />{source}</button>)}
                    {item.provenance && <span className="rounded-full bg-gray-50 px-2.5 py-1 text-xs text-gray-500" title={JSON.stringify(item.provenance, null, 2)}>{language === 'en-US' ? 'Provenance available' : '可查看来源记录'}</span>}
                  </div>
                </div>
                <div className="flex shrink-0 gap-1">
                  {isEditing ? <><button type="button" className="rounded p-2 text-blue-700 hover:bg-blue-50" aria-label="Save memory" onClick={() => void save()}><Save size={16} /></button><button type="button" className="rounded p-2 text-gray-500 hover:bg-gray-100" aria-label="Cancel editing" onClick={() => { setEditing(null); setDraft('') }}><X size={16} /></button></> : <button type="button" className="rounded p-2 text-gray-500 hover:bg-gray-100" aria-label="Edit memory" onClick={() => { setEditing(item); setDraft(item.text) }}><Edit3 size={16} /></button>}
                  <button type="button" className="rounded p-2 text-gray-500 hover:bg-red-50 hover:text-red-700" aria-label="Forget memory" onClick={() => void forget(item)}><Trash2 size={16} /></button>
                </div>
              </div>
            </article>
          )
        })}
      </div>

    </section>
  )
}
