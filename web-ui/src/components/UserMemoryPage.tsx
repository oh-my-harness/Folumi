import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import {
  AlertCircle,
  Bot,
  Brain,
  Check,
  Clock3,
  Download,
  History,
  Pencil,
  Pin,
  PinOff,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react'

type MemoryKind = 'fact' | 'preference' | 'goal' | 'continuity'
type MemoryStatus = 'active' | 'resolved' | 'superseded'
type MemoryPriority = 'normal' | 'pinned'

interface MemorySourceRef {
  source_type: string
  source_id: string
  source_revision?: string | null
}

interface MemoryItem {
  id: string
  kind: MemoryKind
  content: string
  topic_key?: string | null
  status: MemoryStatus
  priority: MemoryPriority
  origin: 'user_explicit' | 'assistant_suggested'
  source_refs: MemorySourceRef[]
  created_at: string
  updated_at: string
  last_confirmed_at: string
  valid_until?: string | null
  resolved_at?: string | null
  revision: string
  supersedes?: string | null
  expired: boolean
}

interface MemorySettings {
  enabled: boolean
  history_recall_enabled: boolean
}

interface Props {
  language: 'zh-CN' | 'en-US'
  assistantName: string
  assistantInstructions: string
  onAssistantProfileChange: (profile: { name: string; instructions: string }) => void
  onSessionNavigate?: (sessionId: string) => void
}

const emptySettings: MemorySettings = { enabled: false, history_recall_enabled: false }

export function UserMemoryPage({
  language,
  assistantName,
  assistantInstructions,
  onAssistantProfileChange,
  onSessionNavigate,
}: Props) {
  const english = language === 'en-US'
  const [activeTab, setActiveTab] = useState<'memory' | 'assistant'>('memory')
  const [items, setItems] = useState<MemoryItem[]>([])
  const [settings, setSettings] = useState<MemorySettings>(emptySettings)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [query, setQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<'effective' | 'all' | MemoryStatus | 'expired'>('effective')
  const [kindFilter, setKindFilter] = useState<'all' | MemoryKind>('all')
  const [showCreate, setShowCreate] = useState(false)
  const [createKind, setCreateKind] = useState<MemoryKind>('preference')
  const [createContent, setCreateContent] = useState('')
  const [createTopic, setCreateTopic] = useState('')
  const [createValidUntil, setCreateValidUntil] = useState('')
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editContent, setEditContent] = useState('')
  const [editTopic, setEditTopic] = useState('')
  const [editKind, setEditKind] = useState<MemoryKind>('preference')
  const [editValidUntil, setEditValidUntil] = useState('')

  const load = async () => {
    setLoading(true)
    setError('')
    try {
      const [itemsResponse, settingsResponse] = await Promise.all([
        fetch('/api/memory/items?include_expired=true'),
        fetch('/api/memory/settings'),
      ])
      if (!itemsResponse.ok || !settingsResponse.ok) throw new Error(english ? 'Failed to load memory.' : '加载记忆失败。')
      const itemsPayload = await itemsResponse.json() as { items?: MemoryItem[] }
      const settingsPayload = await settingsResponse.json() as MemorySettings
      setItems(itemsPayload.items ?? [])
      setSettings(settingsPayload)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { void load() }, [language])

  const visibleItems = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return items.filter((item) => {
      const matchesKind = kindFilter === 'all' || item.kind === kindFilter
      const matchesStatus = statusFilter === 'all'
        || (statusFilter === 'effective' && item.status === 'active' && !item.expired)
        || (statusFilter === 'expired' && item.expired)
        || item.status === statusFilter
      const matchesQuery = !normalizedQuery
        || item.content.toLowerCase().includes(normalizedQuery)
        || item.topic_key?.toLowerCase().includes(normalizedQuery)
      return matchesKind && matchesStatus && matchesQuery
    })
  }, [items, kindFilter, query, statusFilter])

  const updateMemorySettings = async (enabled: boolean) => {
    setBusy(true)
    setError('')
    try {
      const response = await fetch('/api/memory/settings', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      })
      if (!response.ok) throw new Error(await apiError(response))
      setSettings(await response.json() as MemorySettings)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  const createItem = async (conflictAction: 'reject' | 'replace' = 'reject') => {
    if (!createContent.trim()) return
    setBusy(true)
    setError('')
    try {
      const response = await fetch('/api/memory/items', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          kind: createKind,
          content: createContent.trim(),
          topic_key: createTopic.trim() || null,
          valid_until: dateToIso(createValidUntil),
          conflict_action: conflictAction,
        }),
      })
      if (response.status === 409 && conflictAction === 'reject') {
        const payload = await response.json() as { code?: string; existing?: MemoryItem }
        if (payload.code === 'memory_conflict' && payload.existing) {
          const confirmed = window.confirm(english
            ? `This topic already has an active memory:\n\n${payload.existing.content}\n\nReplace it with the new item?`
            : `这个主题已有一条有效记忆：\n\n${payload.existing.content}\n\n是否用新内容替代它？`)
          setBusy(false)
          if (confirmed) await createItem('replace')
          return
        }
      }
      if (!response.ok) throw new Error(await apiError(response))
      const item = await response.json() as MemoryItem
      setItems((current) => [item, ...current.map((old) => old.id === item.supersedes ? { ...old, status: 'superseded' as MemoryStatus } : old)])
      setCreateContent('')
      setCreateTopic('')
      setCreateValidUntil('')
      setShowCreate(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  const patchItem = async (item: MemoryItem, patch: Record<string, unknown>) => {
    setBusy(true)
    setError('')
    try {
      const response = await fetch(`/api/memory/items/${item.id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ revision: item.revision, ...patch }),
      })
      if (!response.ok) throw new Error(await apiError(response))
      const updated = await response.json() as MemoryItem
      setItems((current) => current.map((entry) => entry.id === updated.id ? updated : entry))
      setEditingId(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      if (String(reason).includes('stale')) void load()
    } finally {
      setBusy(false)
    }
  }

  const forgetItem = async (item: MemoryItem) => {
    if (!window.confirm(english
      ? 'Forget this memory permanently? Its content and recoverable revisions will be removed.'
      : '确定永久遗忘这条记忆吗？正文和可恢复历史版本都会被移除。')) return
    setBusy(true)
    setError('')
    try {
      const response = await fetch(`/api/memory/items/${item.id}`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ revision: item.revision }),
      })
      if (!response.ok) throw new Error(await apiError(response))
      setItems((current) => current.filter((entry) => entry.id !== item.id))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  const exportMemory = async () => {
    setBusy(true)
    setError('')
    try {
      const response = await fetch('/api/memory/export.json')
      if (!response.ok) throw new Error(await apiError(response))
      downloadBlob(await response.blob(), 'folumi-memory-export.json')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  const startEdit = (item: MemoryItem) => {
    setEditingId(item.id)
    setEditContent(item.content)
    setEditTopic(item.topic_key ?? '')
    setEditKind(item.kind)
    setEditValidUntil(isoToDateInput(item.valid_until))
  }

  return (
    <section className="h-full overflow-y-auto bg-white px-6 py-6">
      <div className="flex items-start gap-3 border-b border-gray-200 pb-5">
        <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700"><Brain size={21} /></span>
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-semibold text-gray-950">{english ? 'Memory' : '记忆'}</h1>
          <p className="mt-1 text-sm text-gray-500">{english ? 'Manage explicit cross-session memory and the assistant profile.' : '管理明确保存的跨会话记忆与助手配置。'}</p>
        </div>
      </div>

      <div className="mt-5 flex gap-1 border-b border-gray-200" role="tablist" aria-label={english ? 'Memory sections' : '记忆页面分区'}>
        <Tab active={activeTab === 'memory'} onClick={() => setActiveTab('memory')} icon={<Brain size={16} />} label={english ? 'Long-term memory' : '长期记忆'} id="memory" />
        <Tab active={activeTab === 'assistant'} onClick={() => setActiveTab('assistant')} icon={<Bot size={16} />} label={english ? 'Assistant profile' : '助手配置'} id="assistant-profile" />
      </div>

      {activeTab === 'memory' ? (
        <div id="memory-panel" role="tabpanel" aria-labelledby="memory-tab" className="mt-6 max-w-5xl space-y-5">
          {error && (
            <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"><AlertCircle className="mt-0.5 shrink-0" size={17} /><span>{error}</span></div>
          )}

          <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div>
                <h2 className="font-semibold text-gray-950">{english ? 'Saved Memory' : '保存的记忆'}</h2>
                <p className="mt-1 max-w-2xl text-sm leading-6 text-gray-500">{english
                  ? 'Only explicit or approved items are recalled. Turning Memory off keeps existing items but removes memory tools from new runs.'
                  : '只有明确新增或确认的条目才会被召回。关闭后保留已有条目，但新 run 不再挂载记忆工具。'}</p>
              </div>
              <label className="flex cursor-pointer items-center gap-3 text-sm font-medium text-gray-700">
                <span>{settings.enabled ? (english ? 'Enabled' : '已开启') : (english ? 'Disabled' : '已关闭')}</span>
                <input className="peer sr-only" type="checkbox" checked={settings.enabled} disabled={busy || loading} onChange={(event) => void updateMemorySettings(event.target.checked)} />
                <span className="relative h-6 w-11 rounded-full bg-gray-300 transition peer-checked:bg-blue-600 peer-disabled:opacity-60 after:absolute after:left-1 after:top-1 after:h-4 after:w-4 after:rounded-full after:bg-white after:transition peer-checked:after:translate-x-5" />
              </label>
            </div>
          </section>

          <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h2 className="font-semibold text-gray-950">{english ? 'Memory items' : '记忆条目'}</h2>
                <p className="mt-1 text-sm text-gray-500">{english ? `${items.length} saved items` : `共 ${items.length} 条`}</p>
              </div>
              <div className="flex gap-2">
                <button className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50" type="button" onClick={() => void exportMemory()} disabled={busy || loading}><Download size={15} />{english ? 'Export' : '导出'}</button>
                <button className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50" type="button" onClick={() => void load()} disabled={loading}><RefreshCw size={15} className={loading ? 'animate-spin' : ''} />{english ? 'Refresh' : '刷新'}</button>
                <button className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" type="button" onClick={() => setShowCreate((value) => !value)}><Plus size={16} />{english ? 'Add memory' : '添加记忆'}</button>
              </div>
            </div>

            {showCreate && (
              <div className="mt-4 grid gap-3 rounded-lg border border-blue-200 bg-blue-50/60 p-4">
                <div className="grid gap-3 md:grid-cols-[160px_1fr]">
                  <Field label={english ? 'Type' : '类型'}><KindSelect value={createKind} onChange={setCreateKind} english={english} /></Field>
                  <Field label={english ? 'Topic key (optional)' : '主题键（可选）'}><input className="input" value={createTopic} onChange={(event) => setCreateTopic(normalizeTopicInput(event.target.value))} placeholder="preferred_response_language" /></Field>
                </div>
                <Field label={english ? 'Content' : '内容'}><textarea className="min-h-24 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-6 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100" maxLength={1200} value={createContent} onChange={(event) => setCreateContent(event.target.value)} /></Field>
                <Field label={english ? 'Valid until (optional)' : '有效期至（可选）'}><input className="input max-w-56" type="date" value={createValidUntil} onChange={(event) => setCreateValidUntil(event.target.value)} /></Field>
                <div className="flex justify-end gap-2">
                  <button className="btn-secondary" type="button" onClick={() => setShowCreate(false)}><X size={15} />{english ? 'Cancel' : '取消'}</button>
                  <button className="btn-primary" type="button" disabled={busy || !createContent.trim()} onClick={() => void createItem()}><Check size={15} />{english ? 'Save' : '保存'}</button>
                </div>
              </div>
            )}

            <div className="mt-4 grid gap-2 md:grid-cols-[1fr_170px_170px]">
              <label className="relative"><Search className="absolute left-3 top-2.5 text-gray-400" size={16} /><input className="input w-full pl-9" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={english ? 'Search memory' : '搜索记忆'} /></label>
              <select className="input" value={kindFilter} onChange={(event) => setKindFilter(event.target.value as typeof kindFilter)}><option value="all">{english ? 'All types' : '全部类型'}</option>{kindOptions(english)}</select>
              <select className="input" value={statusFilter} onChange={(event) => setStatusFilter(event.target.value as typeof statusFilter)}>
                <option value="effective">{english ? 'Effective' : '有效'}</option><option value="all">{english ? 'All states' : '全部状态'}</option><option value="resolved">{english ? 'Resolved' : '已完成'}</option><option value="superseded">{english ? 'Superseded' : '已替代'}</option><option value="expired">{english ? 'Expired' : '已过期'}</option>
              </select>
            </div>

            <div className="mt-4 space-y-3">
              {loading ? <Empty text={english ? 'Loading memory…' : '正在加载记忆…'} /> : visibleItems.length === 0 ? <Empty text={english ? 'No memory items match this view.' : '当前视图中没有记忆条目。'} /> : visibleItems.map((item) => (
                <article key={item.id} className={`rounded-lg border p-4 ${item.status === 'active' && !item.expired ? 'border-gray-200' : 'border-gray-200 bg-gray-50/70'}`}>
                  {editingId === item.id ? (
                    <div className="grid gap-3">
                      <div className="grid gap-3 md:grid-cols-[160px_1fr]"><KindSelect value={editKind} onChange={setEditKind} english={english} /><input className="input" value={editTopic} onChange={(event) => setEditTopic(normalizeTopicInput(event.target.value))} placeholder={english ? 'Topic key (optional)' : '主题键（可选）'} /></div>
                      <textarea className="min-h-24 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-6" maxLength={1200} value={editContent} onChange={(event) => setEditContent(event.target.value)} />
                      <Field label={english ? 'Valid until (optional)' : '有效期至（可选）'}><input className="input max-w-56" type="date" value={editValidUntil} onChange={(event) => setEditValidUntil(event.target.value)} /></Field>
                      {item.expired && <p className="text-xs text-amber-700">{english ? 'Choose a future date, or clear the date, to renew this expired memory.' : '请选择新的未来日期，或清空日期，以重新确认这条过期记忆。'}</p>}
                      <div className="flex justify-end gap-2"><button className="btn-secondary" type="button" onClick={() => setEditingId(null)}>{english ? 'Cancel' : '取消'}</button><button className="btn-primary" type="button" disabled={busy || !editContent.trim() || isPastDate(editValidUntil)} onClick={() => void patchItem(item, { content: editContent.trim(), kind: editKind, topic_key: editTopic.trim() || undefined, clear_topic_key: !editTopic.trim(), valid_until: dateToIso(editValidUntil), clear_valid_until: !editValidUntil, reconfirm: true })}>{english ? 'Save and reconfirm' : '保存并重新确认'}</button></div>
                    </div>
                  ) : (
                    <>
                      <div className="flex items-start justify-between gap-4">
                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2"><Badge text={kindLabel(item.kind, english)} /><Badge text={stateLabel(item, english)} tone={item.expired || item.status !== 'active' ? 'muted' : 'blue'} />{item.priority === 'pinned' && <Badge text={english ? 'Pinned' : '已置顶'} tone="violet" />}{item.topic_key && <code className="rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-500">{item.topic_key}</code>}</div>
                          <p className="mt-3 whitespace-pre-wrap text-sm leading-6 text-gray-900">{item.content}</p>
                          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-gray-400"><span>{english ? 'Confirmed' : '确认于'} {formatDate(item.last_confirmed_at, language)}</span><span>{english ? 'Updated' : '更新于'} {formatDate(item.updated_at, language)}</span>{item.valid_until && <span className="inline-flex items-center gap-1"><Clock3 size={12} />{english ? 'Valid until' : '有效期至'} {formatDate(item.valid_until, language)}</span>}<span>{item.origin === 'assistant_suggested' ? (english ? 'Assistant suggestion, approved' : '助手建议，经确认') : (english ? 'Added by user' : '用户明确添加')}</span></div>
                          {item.source_refs.length > 0 && <details className="mt-2 text-xs text-gray-500"><summary className="cursor-pointer">{english ? `${item.source_refs.length} source reference(s)` : `${item.source_refs.length} 个来源`}</summary><ul className="mt-1 space-y-1 pl-4">{item.source_refs.map((source, index) => <li key={`${source.source_id}-${index}`}>{source.source_type === 'session' && onSessionNavigate ? <button type="button" className="text-blue-600 hover:underline" onClick={() => onSessionNavigate(source.source_id)}>{english ? 'Open source conversation' : '打开来源会话'} · {source.source_id}</button> : <>{source.source_type}: {source.source_id}</>}</li>)}</ul></details>}
                        </div>
                        <div className="flex shrink-0 flex-wrap justify-end gap-1">
                          <IconButton title={item.priority === 'pinned' ? (english ? 'Unpin' : '取消置顶') : (english ? 'Pin' : '置顶')} onClick={() => void patchItem(item, { priority: item.priority === 'pinned' ? 'normal' : 'pinned' })}>{item.priority === 'pinned' ? <PinOff size={15} /> : <Pin size={15} />}</IconButton>
                          {item.status !== 'superseded' && <IconButton title={english ? 'Edit' : '编辑'} onClick={() => startEdit(item)}><Pencil size={15} /></IconButton>}
                          {item.status !== 'superseded' && <IconButton title={item.expired ? (english ? 'Renew expired memory' : '续期并重新确认') : (english ? 'Reconfirm' : '重新确认')} onClick={() => item.expired ? startEdit(item) : void patchItem(item, { reconfirm: true })}><RefreshCw size={15} /></IconButton>}
                          {matchesResolvable(item.kind) && item.status !== 'superseded' && <IconButton title={item.status === 'resolved' ? (english ? 'Reopen' : '重新打开') : (english ? 'Mark resolved' : '标记完成')} onClick={() => void patchItem(item, { status: item.status === 'resolved' ? 'active' : 'resolved' })}><Check size={15} /></IconButton>}
                          <IconButton danger title={english ? 'Forget permanently' : '永久遗忘'} onClick={() => void forgetItem(item)}><Trash2 size={15} /></IconButton>
                        </div>
                      </div>
                    </>
                  )}
                </article>
              ))}
            </div>
          </section>

          <section className="rounded-xl border border-gray-200 bg-gray-50 p-5">
            <div className="flex items-start gap-3"><span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-gray-200 text-gray-600"><History size={18} /></span><div><div className="flex flex-wrap items-center gap-2"><h2 className="font-semibold text-gray-900">{english ? 'History Recall' : '历史检索'}</h2><Badge text={english ? 'Not available yet' : '暂未开放'} tone="muted" /></div><p className="mt-1 text-sm leading-6 text-gray-500">{english ? 'The runtime does not yet provide the required cross-session recall projection. This stays off instead of creating a second conversation store.' : 'runtime 尚未提供所需的跨会话检索投影。该能力保持关闭，不在产品侧复制第二套会话仓库。'}</p></div></div>
          </section>
        </div>
      ) : (
        <section id="assistant-profile-panel" role="tabpanel" aria-labelledby="assistant-profile-tab" className="mt-6 max-w-3xl rounded-lg border border-gray-200 bg-white p-5">
          <div className="flex items-start gap-3"><span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-violet-50 text-violet-700"><Bot size={19} /></span><div><h2 className="font-semibold text-gray-950">{english ? 'Assistant profile' : '助手配置'}</h2><p className="mt-1 text-sm leading-6 text-gray-500">{english ? 'Define the identity and behavior shared by new conversations.' : '定义所有新会话共用的助手身份与行为偏好。'}</p></div></div>
          <div className="mt-4 grid gap-4"><Field label={english ? 'Assistant name' : '助手名称'}><input className="input" value={assistantName} onChange={(event) => onAssistantProfileChange({ name: event.target.value, instructions: assistantInstructions })} /></Field><Field label={english ? 'Behavior instructions' : '行为说明'}><textarea className="min-h-28 resize-y rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-6 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100" value={assistantInstructions} onChange={(event) => onAssistantProfileChange({ name: assistantName, instructions: event.target.value })} /></Field><p className="text-xs text-gray-400">{english ? 'Changes are saved automatically. Memory switches do not disable this profile.' : '更改会自动保存；Memory 开关不会禁用助手配置。'}</p></div>
        </section>
      )}
    </section>
  )
}

function Tab({ active, onClick, icon, label, id }: { active: boolean; onClick: () => void; icon: ReactNode; label: string; id: string }) {
  return <button type="button" role="tab" id={`${id}-tab`} aria-controls={`${id}-panel`} aria-selected={active} className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${active ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`} onClick={onClick}>{icon}{label}</button>
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="grid gap-1.5 text-sm font-medium text-gray-800">{label}{children}</label>
}

function KindSelect({ value, onChange, english }: { value: MemoryKind; onChange: (value: MemoryKind) => void; english: boolean }) {
  return <select className="input" value={value} onChange={(event) => onChange(event.target.value as MemoryKind)}>{kindOptions(english)}</select>
}

function kindOptions(english: boolean) {
  return <><option value="fact">{english ? 'Fact' : '事实'}</option><option value="preference">{english ? 'Preference' : '偏好'}</option><option value="goal">{english ? 'Goal' : '目标'}</option><option value="continuity">{english ? 'Continuity' : '连续性事项'}</option></>
}

function IconButton({ title, onClick, danger = false, children }: { title: string; onClick: () => void; danger?: boolean; children: ReactNode }) {
  return <button type="button" title={title} aria-label={title} className={`flex h-8 w-8 items-center justify-center rounded-md border ${danger ? 'border-red-200 text-red-600 hover:bg-red-50' : 'border-gray-200 text-gray-500 hover:bg-gray-50 hover:text-gray-800'}`} onClick={onClick}>{children}</button>
}

function Badge({ text, tone = 'blue' }: { text: string; tone?: 'blue' | 'muted' | 'violet' }) {
  const colors = tone === 'muted' ? 'bg-gray-100 text-gray-600' : tone === 'violet' ? 'bg-violet-50 text-violet-700' : 'bg-blue-50 text-blue-700'
  return <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${colors}`}>{text}</span>
}

function Empty({ text }: { text: string }) {
  return <div className="rounded-lg border border-dashed border-gray-300 px-4 py-10 text-center text-sm text-gray-500">{text}</div>
}

function kindLabel(kind: MemoryKind, english: boolean) {
  return ({ fact: english ? 'Fact' : '事实', preference: english ? 'Preference' : '偏好', goal: english ? 'Goal' : '目标', continuity: english ? 'Continuity' : '连续性事项' })[kind]
}

function stateLabel(item: MemoryItem, english: boolean) {
  if (item.expired) return english ? 'Expired' : '已过期'
  return ({ active: english ? 'Active' : '有效', resolved: english ? 'Resolved' : '已完成', superseded: english ? 'Superseded' : '已替代' })[item.status]
}

function matchesResolvable(kind: MemoryKind) {
  return kind === 'goal' || kind === 'continuity'
}

function normalizeTopicInput(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9._-]/g, '')
}

function dateToIso(value: string) {
  return value ? new Date(`${value}T23:59:59`).toISOString() : null
}

function isoToDateInput(value?: string | null) {
  if (!value) return ''
  const date = new Date(value)
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

function isPastDate(value: string) {
  const iso = dateToIso(value)
  return Boolean(iso && new Date(iso).getTime() <= Date.now())
}

function formatDate(value: string, language: 'zh-CN' | 'en-US') {
  return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

async function apiError(response: Response) {
  try {
    const payload = await response.json() as { error?: string; code?: string }
    return payload.error || payload.code || `HTTP ${response.status}`
  } catch {
    return `HTTP ${response.status}`
  }
}

function downloadBlob(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = fileName
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}
