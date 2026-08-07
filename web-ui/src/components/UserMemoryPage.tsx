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
  Trash2,
  X,
} from 'lucide-react'
import { defaultLlmSettings } from '../settings'

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
  assistant_write_without_approval: boolean
}

interface Props {
  language: 'zh-CN' | 'en-US'
  assistantName: string
  assistantInstructions: string
  onAssistantProfileChange: (profile: { name: string; instructions: string }) => void
  onSessionNavigate?: (sessionId: string) => void
}

const emptySettings: MemorySettings = { enabled: false, history_recall_enabled: false, assistant_write_without_approval: false }

export function UserMemoryPage({
  language,
  assistantName,
  assistantInstructions,
  onAssistantProfileChange,
  onSessionNavigate,
}: Props) {
  const english = language === 'en-US'
  const usesDefaultAssistantInstructions = !assistantInstructions.trim()
  const customNameWithDefaultIdentity = usesDefaultAssistantInstructions
    && Boolean(assistantName.trim())
    && assistantName.trim() !== defaultLlmSettings.assistantName
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
      if (!itemsResponse.ok || !settingsResponse.ok) throw new Error(english ? 'Failed to load personal information.' : '加载个人信息失败。')
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

  const updateMemorySettings = async (patch: Partial<MemorySettings>) => {
    setBusy(true)
    setError('')
    try {
      const response = await fetch('/api/memory/settings', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
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
            ? `This topic already has active personal information:\n\n${payload.existing.content}\n\nReplace it with the new item?`
            : `这个主题已有一条有效个人信息：\n\n${payload.existing.content}\n\n是否用新内容替代它？`)
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
      ? 'Remove this personal information permanently? Its content and recoverable revisions will be deleted.'
      : '确定永久移除这条个人信息吗？正文和可恢复历史版本都会被删除。')) return
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
          <h1 className="text-xl font-semibold text-gray-950">{english ? 'Personalization' : '个性化'}</h1>
          <p className="mt-1 text-sm text-gray-500">{english ? 'Decide how the Assistant knows you, references past conversations, and presents itself.' : '决定助手如何认识你、参考过往对话并表现自己。'}</p>
        </div>
      </div>

      <div className="mt-5 flex gap-1 border-b border-gray-200" role="tablist" aria-label={english ? 'Personalization sections' : '个性化页面分区'}>
        <Tab active={activeTab === 'memory'} onClick={() => setActiveTab('memory')} icon={<Brain size={16} />} label={english ? 'About me' : '关于我'} id="memory" />
        <Tab active={activeTab === 'assistant'} onClick={() => setActiveTab('assistant')} icon={<Bot size={16} />} label={english ? 'Assistant setup' : '助手设定'} id="assistant-profile" />
      </div>

      {activeTab === 'memory' ? (
        <div id="memory-panel" role="tabpanel" aria-labelledby="memory-tab" className="mt-6 max-w-5xl space-y-5">
          {error && (
            <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800"><AlertCircle className="mt-0.5 shrink-0" size={17} /><span>{error}</span></div>
          )}

          <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
            <div>
              <h2 className="font-semibold text-gray-950">{english ? 'Personal context' : '个人信息与对话'}</h2>
              <p className="mt-1 max-w-3xl text-sm leading-6 text-gray-500">{english
                ? 'Control personal information, assistant updates, and on-demand access to past conversations in one place.'
                : '集中管理个人信息、助手自动补充和按需参考过往对话。'}</p>
            </div>

            <div className="mt-4 divide-y divide-gray-100 rounded-lg border border-gray-100 bg-gray-50/50 px-4">
              <SettingRow
                icon={<Brain size={17} />}
                iconClass="bg-blue-100 text-blue-700"
                title={english ? 'Use personal information' : '使用个人信息'}
                description={english
                  ? 'Use active personal details across conversations. Turning this off keeps existing items but stops new conversations from accessing them.'
                  : '让助手在不同会话中使用有效的个人信息。关闭后仍保留已有内容，但新对话不再访问。'}
                state={settings.enabled ? (english ? 'Enabled' : '已开启') : (english ? 'Disabled' : '已关闭')}
                checked={settings.enabled}
                disabled={busy || loading}
                onChange={(checked) => void updateMemorySettings({ enabled: checked })}
              />
              <SettingRow
                icon={<Bot size={17} />}
                iconClass="bg-violet-100 text-violet-700"
                title={english ? 'Allow assistant updates' : '允许助手自动补充'}
                description={english
                  ? 'Allow the Assistant to save clearly durable details you directly share without asking each time. Sensitive, inferred, and transient details remain excluded; permanent removal still requires approval.'
                  : '允许助手直接保存你明确透露的姓名、稳定偏好或长期目标等持久信息。敏感、推断和临时信息仍不得主动保存；永久移除仍需审批。'}
                state={settings.assistant_write_without_approval ? (english ? 'Allowed' : '已授权') : (english ? 'Approval required' : '需要审批')}
                checked={settings.assistant_write_without_approval}
                disabled={busy || loading || !settings.enabled}
                onChange={(checked) => void updateMemorySettings({ assistant_write_without_approval: checked })}
                warning={!settings.enabled ? (english ? 'Turn on personal information first.' : '请先开启“使用个人信息”。') : undefined}
              />
              <SettingRow
                icon={<History size={17} />}
                iconClass="bg-cyan-100 text-cyan-700"
                title={english ? 'Reference past conversations' : '参考过往对话'}
                description={english
                  ? 'Search eligible conversations only when earlier context is needed. Conversations are not read secretly before every answer, and results do not become personal information automatically.'
                  : '仅在需要过往上下文时按需搜索符合权限的会话；不会在每次回答前偷偷读取，找到的内容也不会自动变成个人信息。'}
                state={settings.history_recall_enabled ? (english ? 'Enabled' : '已开启') : (english ? 'Disabled' : '已关闭')}
                checked={settings.history_recall_enabled}
                disabled={busy || loading || !settings.enabled}
                onChange={(checked) => void updateMemorySettings({ history_recall_enabled: checked })}
                warning={!settings.enabled ? (english ? 'Turn on personal information first.' : '请先开启“使用个人信息”。') : undefined}
              />
            </div>
          </section>

          <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h2 className="font-semibold text-gray-950">{english ? 'Personal information' : '个人信息'}</h2>
                <p className="mt-1 text-sm text-gray-500">{english ? `${items.length} saved items` : `共 ${items.length} 条`}</p>
              </div>
              <div className="flex gap-2">
                <button className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50" type="button" onClick={() => void exportMemory()} disabled={busy || loading}><Download size={15} />{english ? 'Export' : '导出'}</button>
                <button className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50" type="button" onClick={() => void load()} disabled={loading}><RefreshCw size={15} className={loading ? 'animate-spin' : ''} />{english ? 'Refresh' : '刷新'}</button>
                <button className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" type="button" onClick={() => setShowCreate((value) => !value)}><Plus size={16} />{english ? 'Add information' : '添加信息'}</button>
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
              <label><span className="sr-only">{english ? 'Search personal information' : '搜索个人信息'}</span><input className="input w-full" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={english ? 'Search personal information' : '搜索个人信息'} /></label>
              <select className="input" value={kindFilter} onChange={(event) => setKindFilter(event.target.value as typeof kindFilter)}><option value="all">{english ? 'All types' : '全部类型'}</option>{kindOptions(english)}</select>
              <select className="input" value={statusFilter} onChange={(event) => setStatusFilter(event.target.value as typeof statusFilter)}>
                <option value="effective">{english ? 'Effective' : '有效'}</option><option value="all">{english ? 'All states' : '全部状态'}</option><option value="resolved">{english ? 'Resolved' : '已完成'}</option><option value="superseded">{english ? 'Superseded' : '已替代'}</option><option value="expired">{english ? 'Expired' : '已过期'}</option>
              </select>
            </div>

            <div className="mt-4 space-y-3">
              {loading ? <Empty text={english ? 'Loading personal information…' : '正在加载个人信息…'} /> : visibleItems.length === 0 ? <Empty text={english ? 'No personal information matches this view.' : '当前视图中没有匹配的个人信息。'} /> : visibleItems.map((item) => (
                <article key={item.id} className={`rounded-lg border p-4 ${item.status === 'active' && !item.expired ? 'border-gray-200' : 'border-gray-200 bg-gray-50/70'}`}>
                  {editingId === item.id ? (
                    <div className="grid gap-3">
                      <div className="grid gap-3 md:grid-cols-[160px_1fr]"><KindSelect value={editKind} onChange={setEditKind} english={english} /><input className="input" value={editTopic} onChange={(event) => setEditTopic(normalizeTopicInput(event.target.value))} placeholder={english ? 'Topic key (optional)' : '主题键（可选）'} /></div>
                      <textarea className="min-h-24 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-6" maxLength={1200} value={editContent} onChange={(event) => setEditContent(event.target.value)} />
                      <Field label={english ? 'Valid until (optional)' : '有效期至（可选）'}><input className="input max-w-56" type="date" value={editValidUntil} onChange={(event) => setEditValidUntil(event.target.value)} /></Field>
                      {item.expired && <p className="text-xs text-amber-700">{english ? 'Choose a future date, or clear the date, to renew this expired information.' : '请选择新的未来日期，或清空日期，以重新确认这条过期信息。'}</p>}
                      <div className="flex justify-end gap-2"><button className="btn-secondary" type="button" onClick={() => setEditingId(null)}>{english ? 'Cancel' : '取消'}</button><button className="btn-primary" type="button" disabled={busy || !editContent.trim() || isPastDate(editValidUntil)} onClick={() => void patchItem(item, { content: editContent.trim(), kind: editKind, topic_key: editTopic.trim() || undefined, clear_topic_key: !editTopic.trim(), valid_until: dateToIso(editValidUntil), clear_valid_until: !editValidUntil, reconfirm: true })}>{english ? 'Save and reconfirm' : '保存并重新确认'}</button></div>
                    </div>
                  ) : (
                    <>
                      <div className="flex items-start justify-between gap-4">
                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2"><Badge text={kindLabel(item.kind, english)} /><Badge text={stateLabel(item, english)} tone={item.expired || item.status !== 'active' ? 'muted' : 'blue'} />{item.priority === 'pinned' && <Badge text={english ? 'Pinned' : '已置顶'} tone="violet" />}{item.topic_key && <code className="rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-500">{item.topic_key}</code>}</div>
                          <p className="mt-3 whitespace-pre-wrap text-sm leading-6 text-gray-900">{item.content}</p>
                          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-gray-400"><span>{english ? 'Recorded' : '记录于'} {formatDate(item.last_confirmed_at, language)}</span><span>{english ? 'Updated' : '更新于'} {formatDate(item.updated_at, language)}</span>{item.valid_until && <span className="inline-flex items-center gap-1"><Clock3 size={12} />{english ? 'Valid until' : '有效期至'} {formatDate(item.valid_until, language)}</span>}<span>{item.origin === 'assistant_suggested' ? (english ? 'Added by Assistant' : '助手添加') : (english ? 'Added by user' : '用户明确添加')}</span></div>
                          {item.source_refs.length > 0 && <details className="mt-2 text-xs text-gray-500"><summary className="cursor-pointer">{english ? `${item.source_refs.length} source reference(s)` : `${item.source_refs.length} 个来源`}</summary><ul className="mt-1 space-y-1 pl-4">{item.source_refs.map((source, index) => <li key={`${source.source_id}-${index}`}>{source.source_type === 'session' && onSessionNavigate ? <button type="button" className="text-blue-600 hover:underline" onClick={() => onSessionNavigate(source.source_id)}>{english ? 'Open source conversation' : '打开来源会话'} · {source.source_id}</button> : <>{source.source_type}: {source.source_id}</>}</li>)}</ul></details>}
                        </div>
                        <div className="flex shrink-0 flex-wrap justify-end gap-1">
                          <IconButton title={item.priority === 'pinned' ? (english ? 'Unpin' : '取消置顶') : (english ? 'Pin' : '置顶')} onClick={() => void patchItem(item, { priority: item.priority === 'pinned' ? 'normal' : 'pinned' })}>{item.priority === 'pinned' ? <PinOff size={15} /> : <Pin size={15} />}</IconButton>
                          {item.status !== 'superseded' && <IconButton title={english ? 'Edit' : '编辑'} onClick={() => startEdit(item)}><Pencil size={15} /></IconButton>}
                          {item.status !== 'superseded' && <IconButton title={item.expired ? (english ? 'Renew expired information' : '续期并重新确认') : (english ? 'Reconfirm' : '重新确认')} onClick={() => item.expired ? startEdit(item) : void patchItem(item, { reconfirm: true })}><RefreshCw size={15} /></IconButton>}
                          {matchesResolvable(item.kind) && item.status !== 'superseded' && <IconButton title={item.status === 'resolved' ? (english ? 'Reopen' : '重新打开') : (english ? 'Mark resolved' : '标记完成')} onClick={() => void patchItem(item, { status: item.status === 'resolved' ? 'active' : 'resolved' })}><Check size={15} /></IconButton>}
                          <IconButton danger title={english ? 'Remove permanently' : '永久移除'} onClick={() => void forgetItem(item)}><Trash2 size={15} /></IconButton>
                        </div>
                      </div>
                    </>
                  )}
                </article>
              ))}
            </div>
          </section>

        </div>
      ) : (
        <section id="assistant-profile-panel" role="tabpanel" aria-labelledby="assistant-profile-tab" className="mt-6 max-w-3xl rounded-lg border border-gray-200 bg-white p-5">
          <div className="flex items-start gap-3"><span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-violet-50 text-violet-700"><Bot size={19} /></span><div><h2 className="font-semibold text-gray-950">{english ? 'Assistant setup' : '助手设定'}</h2><p className="mt-1 text-sm leading-6 text-gray-500">{english ? 'Define the identity and behavior shared by new conversations.' : '定义所有新会话共用的助手身份与行为偏好。'}</p></div></div>
          <div className="mt-4 grid gap-4">
            <Field label={english ? 'Assistant name' : '助手名称'}>
              <input className="input" value={assistantName} onChange={(event) => onAssistantProfileChange({ name: event.target.value, instructions: assistantInstructions })} />
              <span className="text-xs font-normal leading-5 text-gray-500">{english ? 'The name and the instructions below are applied together. If they conflict, the instructions may determine how the Assistant identifies itself.' : '助手名称会与下方说明一起生效；如果两者冲突，助手可能优先按照说明介绍自己的身份。'}</span>
            </Field>
            <Field label={english ? 'Identity and behavior instructions' : '身份与行为说明'}>
              <textarea
                className="min-h-28 resize-y rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-6 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100"
                value={assistantInstructions}
                placeholder={defaultLlmSettings.assistantInstructions}
                onChange={(event) => onAssistantProfileChange({ name: assistantName, instructions: event.target.value })}
              />
              <span className="text-xs font-normal leading-5 text-gray-500">{english ? 'Leave this empty to use the default Folumi identity and behavior shown in the placeholder.' : '留空时，运行时会使用输入框占位内容所示的默认 Folumi 身份与行为说明。'}</span>
            </Field>
            {usesDefaultAssistantInstructions && (
              <div className={`flex items-start gap-2 rounded-lg border px-3 py-3 text-sm ${customNameWithDefaultIdentity ? 'border-amber-200 bg-amber-50 text-amber-900' : 'border-blue-200 bg-blue-50 text-blue-900'}`} role="status">
                <AlertCircle className="mt-0.5 shrink-0" size={17} />
                <div className="min-w-0">
                  <p className="font-medium">{customNameWithDefaultIdentity
                    ? english ? `The name is “${assistantName.trim()}”, but the empty instructions will use the default Folumi identity.` : `当前名称是“${assistantName.trim()}”，但说明为空，实际运行时仍会使用默认 Folumi 身份。`
                    : english ? 'Default Folumi instructions are currently in effect.' : '当前实际生效的是默认 Folumi 身份说明。'}</p>
                  <p className="mt-1 whitespace-pre-wrap text-xs font-normal leading-5 opacity-80">{defaultLlmSettings.assistantInstructions}</p>
                </div>
              </div>
            )}
            <p className="text-xs text-gray-400">{english ? 'Changes are saved automatically and apply to new conversations only. Existing conversations keep the setup captured when they were created. Personal information controls do not disable this setup.' : '更改会自动保存，但只应用于新会话；已有会话继续使用创建时保存的设定。“关于我”中的开关不会禁用助手设定。'}</p>
          </div>
        </section>
      )}
    </section>
  )
}

function Tab({ active, onClick, icon, label, id }: { active: boolean; onClick: () => void; icon: ReactNode; label: string; id: string }) {
  return <button type="button" role="tab" id={`${id}-tab`} aria-controls={`${id}-panel`} aria-selected={active} className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${active ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`} onClick={onClick}>{icon}{label}</button>
}

function SettingRow({
  icon,
  iconClass,
  title,
  description,
  state,
  checked,
  disabled,
  onChange,
  warning,
}: {
  icon: ReactNode
  iconClass: string
  title: string
  description: string
  state: string
  checked: boolean
  disabled: boolean
  onChange: (checked: boolean) => void
  warning?: string
}) {
  return (
    <div className="grid gap-4 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="flex min-w-0 items-start gap-3">
        <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg ${iconClass}`}>{icon}</span>
        <div className="min-w-0">
          <h3 className="text-sm font-medium text-gray-900">{title}</h3>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-gray-500">{description}</p>
          {warning && <p className="mt-1.5 text-xs text-amber-700">{warning}</p>}
        </div>
      </div>
      <label className={`flex min-w-[148px] items-center justify-end gap-3 text-sm font-medium ${disabled ? 'cursor-not-allowed text-gray-400' : 'cursor-pointer text-gray-700'}`}>
        <span>{state}</span>
        <input
          type="checkbox"
          className="peer sr-only"
          aria-label={title}
          checked={checked}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="relative h-6 w-11 rounded-full bg-gray-300 transition peer-checked:bg-blue-600 peer-disabled:opacity-60 after:absolute after:left-1 after:top-1 after:h-4 after:w-4 after:rounded-full after:bg-white after:transition peer-checked:after:translate-x-5" />
      </label>
    </div>
  )
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
