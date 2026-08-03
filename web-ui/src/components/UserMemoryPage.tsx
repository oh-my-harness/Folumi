import { useCallback, useEffect, useState } from 'react'
import { Bot, Brain, Edit3, ExternalLink, RefreshCw, Save, Trash2, X } from 'lucide-react'
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
  assistantName: string
  assistantInstructions: string
  onEnabledChange: (enabled: boolean) => void
  onAssistantProfileChange: (profile: { name: string; instructions: string }) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}

export function UserMemoryPage({
  language,
  enabled,
  assistantName,
  assistantInstructions,
  onEnabledChange,
  onAssistantProfileChange,
  onSourceNavigate,
}: Props) {
  const [items, setItems] = useState<MemoryItem[]>([])
  const [loading, setLoading] = useState(true)
  const [status, setStatus] = useState('')
  const [editing, setEditing] = useState<MemoryItem | null>(null)
  const [draft, setDraft] = useState('')
  const [activeTab, setActiveTab] = useState<'memory' | 'assistant'>('memory')

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
          <p className="mt-1 text-sm text-gray-500">{language === 'en-US' ? 'Configure the assistant and control the long-term context it may carry across conversations.' : '配置助手，并控制它可以跨会话延续的长期信息。'}</p>
        </div>
      </div>

      <div className="mt-5 flex gap-1 border-b border-gray-200" role="tablist" aria-label={language === 'en-US' ? 'Memory sections' : '记忆页面分区'}>
        <button
          type="button"
          role="tab"
          id="memory-tab"
          aria-controls="memory-panel"
          aria-selected={activeTab === 'memory'}
          className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${activeTab === 'memory' ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`}
          onClick={() => setActiveTab('memory')}
        >
          <Brain size={16} />
          {language === 'en-US' ? 'Long-term memory' : '长期记忆'}
        </button>
        <button
          type="button"
          role="tab"
          id="assistant-profile-tab"
          aria-controls="assistant-profile-panel"
          aria-selected={activeTab === 'assistant'}
          className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${activeTab === 'assistant' ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`}
          onClick={() => setActiveTab('assistant')}
        >
          <Bot size={16} />
          {language === 'en-US' ? 'Assistant profile' : '助手配置'}
        </button>
      </div>

      {activeTab === 'memory' ? (
        <div id="memory-panel" role="tabpanel" aria-labelledby="memory-tab">
          <div className="mt-6 flex flex-wrap items-start justify-between gap-4">
            <div>
              <h2 className="font-semibold text-gray-950">{language === 'en-US' ? 'Long-term memory' : '长期记忆'}</h2>
              <p className="mt-1 text-sm text-gray-500">{language === 'en-US' ? 'Inspect, correct, or forget information retained across conversations.' : '检查、修正或遗忘跨会话保留的信息。'}</p>
            </div>
            <div className="flex flex-wrap gap-2">
              <label className="flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm text-gray-700">
                <input type="checkbox" checked={enabled} onChange={(event) => onEnabledChange(event.target.checked)} />
                {language === 'en-US' ? 'Use memory' : '启用记忆'}
              </label>
              <button type="button" className="inline-flex h-9 items-center gap-2 rounded-lg border border-gray-200 px-3 text-sm text-gray-700" disabled={loading} onClick={() => void load()}><RefreshCw size={15} className={loading ? 'animate-spin' : ''} />{language === 'en-US' ? 'Refresh' : '刷新'}</button>
            </div>
          </div>

          <div className={`mt-4 rounded-lg px-4 py-3 text-sm ${enabled ? 'bg-emerald-50 text-emerald-800' : 'bg-gray-100 text-gray-600'}`}>
            {enabled
              ? (language === 'en-US' ? 'Memory is on for new sessions.' : '新会话已启用长期记忆。')
              : (language === 'en-US' ? 'Memory is off. Existing items remain here until you edit or forget them.' : '记忆已关闭；已有内容会保留，直到你编辑或遗忘。')}
          </div>
          {status && <div className="mt-4 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{status}</div>}

          <div className="mt-3 space-y-3">
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
        </div>
      ) : (
        <section id="assistant-profile-panel" role="tabpanel" aria-labelledby="assistant-profile-tab" className="mt-6 max-w-3xl rounded-lg border border-gray-200 bg-white p-5">
          <div className="flex items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-violet-50 text-violet-700"><Bot size={19} /></span>
            <div>
              <h2 className="font-semibold text-gray-950">{language === 'en-US' ? 'Assistant profile' : '助手配置'}</h2>
              <p className="mt-1 text-sm leading-6 text-gray-500">
                {language === 'en-US'
                  ? 'Define the identity and behavior shared by new conversations. These instructions cannot override safety or data permissions.'
                  : '定义所有新会话共用的助手身份与行为偏好；这些说明不能覆盖安全规则或数据权限。'}
              </p>
            </div>
          </div>
          <div className="mt-4 grid gap-4">
            <label className="grid gap-1.5 text-sm font-medium text-gray-800">
              {language === 'en-US' ? 'Assistant name' : '助手名称'}
              <input
                className="h-10 rounded-md border border-gray-300 bg-white px-3 text-sm font-normal outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100"
                value={assistantName}
                onChange={(event) => onAssistantProfileChange({ name: event.target.value, instructions: assistantInstructions })}
              />
            </label>
            <label className="grid gap-1.5 text-sm font-medium text-gray-800">
              {language === 'en-US' ? 'Behavior instructions' : '行为说明'}
              <textarea
                className="min-h-28 resize-y rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-normal leading-6 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100"
                value={assistantInstructions}
                onChange={(event) => onAssistantProfileChange({ name: assistantName, instructions: event.target.value })}
                placeholder={language === 'en-US'
                  ? 'For example: be concise, distinguish facts from suggestions, and prefer my saved terminology.'
                  : '例如：回答简洁，区分事实与建议，优先使用我笔记中的术语。'}
              />
            </label>
            <p className="text-xs text-gray-400">{language === 'en-US' ? 'Changes are saved automatically.' : '更改会自动保存。'}</p>
          </div>
        </section>
      )}

    </section>
  )
}
