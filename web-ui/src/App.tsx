import { useState, useCallback, useEffect, useMemo, useRef } from 'react'
import type { Dispatch, SetStateAction } from 'react'
import { ChatBox } from './components/ChatBox'
import type { ChatAttachment, ContextStats, NotebookEditProposal, SaveToNotebookOptions } from './components/ChatBox'
import { TracePanel, TraceEntry } from './components/TracePanel'
import { SettingsPage, type SettingsTab } from './components/SettingsPage'
import { KnowledgeBasePage } from './components/KnowledgeBasePage'
import { NotesPage } from './components/NotesPage'
import { UserMemoryPage } from './components/UserMemoryPage'
import { OnboardingDialog } from './components/OnboardingDialog'
import { OnboardingResumeButton } from './components/OnboardingResumeButton'
import { AppTitleBar } from './components/AppTitleBar'
import { AppView, Sidebar } from './components/Sidebar'
import type { DeepSolveTraceEntry } from './components/DeepSolveMessage'
import type { SourceReference, SourceTarget } from './components/MarkdownMessage'
import { AgentStatus } from './agentStatus'
import { useWebSocket } from './hooks/useWebSocket'
import {
  DEFAULT_CONTEXT_WINDOW_TOKENS,
  CURRENT_ONBOARDING_VERSION,
  activeLlmConfig,
  completeOnboardingSettings,
  hasLocalLlmSettings,
  loadLlmSettings,
  loadStoredLlmSettings,
  saveLlmSettings,
  saveStoredLlmSettings,
  searchForSession,
  shouldShowOnboarding,
  settingsRequireSessionReset,
  settingsForSession,
} from './settings'
import { guideAssistantStarterPrompt, type ProductGuideDestination } from './productGuide'
import {
  appendCompletedSessionMessage,
  completedStreamText,
  isCurrentSessionEvent,
  isLatestSessionHydration,
  reconcileSessionMessages,
  reconcileSessionRunState,
} from './sessionResilience'
import { I18nProvider, translate, type TranslationKey } from './i18n'
import { openExternalUrl, setNativeWindowTheme } from './api'
import {
  normalizeNotebookEntryPath,
  normalizeNotebookFolderPath,
  notebookFileNameFromTitle,
  resolveGeneratedNotebookEntryType,
  titleFromMarkdown,
} from './notebookSave'
import type { NotebookVaultInfo, SaveToNotebookResult } from './notebookSave'
import { knowledgeCitationsFromTrace } from './knowledgeCitation'

type Capability = 'chat' | 'deep_solve' | 'code_exec' | 'organize'

interface Message {
  role: 'user' | 'assistant' | 'status'
  text: string
  kind?: AgentStatus['kind']
  transient?: boolean
  citations?: Citation[]
  deepSolve?: DeepSolveTraceEntry[]
  notebookEditProposal?: NotebookEditProposal
  attachments?: ChatAttachment[]
}

interface Citation {
  index: number
  source: string
  text: string
  kind?: 'rag' | 'web'
  title?: string
  url?: string
  score?: number | null
  kb?: string
  documentId?: string
  chunkId?: string
  rawSource?: string
  page?: string | number
}

interface RecentSession {
  id: string
  title: string
  activeRun?: SessionRunSummary | null
  pinned?: boolean
  temporary?: boolean
}

interface SessionRunSummary {
  run_id?: string
  session_id?: string
  capability?: Capability
  status?: string
  current_stage?: string | null
  started_at?: string
  updated_at?: string
}

interface KnowledgeBaseOption {
  id: string
  name: string
}

interface SessionListResponse {
  sessions?: Array<{
    id: string
    title?: string
    name?: string | null
    active_run?: SessionRunSummary | null
    temporary?: boolean
  }>
}

interface SessionDetailResponse {
  capability?: Capability
  kb?: string | null
  kbs?: string[]
  notebook_enabled?: boolean
  notebook_vault_id?: string | null
  temporary?: boolean
  llm?: { model?: string | null } | null
  messages?: Array<{
    role: 'user' | 'assistant'
    text: string
    citations?: Citation[]
  }>
  trace?: Array<{
    kind: string
    timestamp?: string
    payload?: Record<string, unknown>
  }>
  compact_summary?: {
    summary: string
    timestamp?: string
    message_count?: number
  } | null
  active_run?: SessionRunSummary | null
  run_state?: SessionRunSummary | null
  latest_usage?: TokenUsagePayload | null
  metadata?: {
    name?: string | null
  }
}

interface TokenUsagePayload {
  input_tokens?: number
  output_tokens?: number
  cache_read_tokens?: number
  cache_creation_tokens?: number
  total_tokens?: number
  source?: string
}

interface MemoryApprovalRequest {
  requestId: string
  sessionId: string
  tool: 'memory_write' | 'memory_forget'
  args: Record<string, unknown>
}

export default function App() {
  const [view, setView] = useState<AppView>('assistant')
  const [capability, setCapability] = useState<Capability>('chat')
  const [llmSettings, setLlmSettings] = useState(loadLlmSettings)
  const [settingsHydrated, setSettingsHydrated] = useState(false)
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const [onboardingStep, setOnboardingStep] = useState(0)
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('llm')
  const [starterDraft, setStarterDraft] = useState<{ id: number; text: string } | null>(null)
  const [selectedLlmConfigId, setSelectedLlmConfigId] = useState<string | null>(() => loadLlmSettings().activeLlmConfigId)
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [temporaryConversation, setTemporaryConversation] = useState(false)
  const activeSessionIdRef = useRef<string | null>(null)
  const sessionSelectionVersionRef = useRef(0)
  const sessionHydrationVersionRef = useRef(0)
  const activateSession = useCallback((id: string | null) => {
    activeSessionIdRef.current = id
    sessionSelectionVersionRef.current += 1
    setSessionId(id)
    return sessionSelectionVersionRef.current
  }, [])
  const [messages, setMessages] = useState<Message[]>([])
  const [streamingText, setStreamingText] = useState('')
  const streamingRef = useRef('')
  const progressStreamingRef = useRef('')
  const pendingSessionSendRef = useRef<{ sessionId: string; content: string } | null>(null)
  const [traceEntries, setTraceEntries] = useState<TraceEntry[]>([])
  const pendingCitationsRef = useRef<Citation[]>([])
  const pendingDeepSolveRef = useRef<DeepSolveTraceEntry[]>([])
  const pendingNotebookEditProposalRef = useRef<NotebookEditProposal | undefined>(undefined)
  const [running, setRunning] = useState(false)
  const [memoryApproval, setMemoryApproval] = useState<MemoryApprovalRequest | null>(null)
  const [recentSessions, setRecentSessions] = useState<RecentSession[]>([])
  const [pinnedSessionIds, setPinnedSessionIds] = useState<Set<string>>(() => loadPinnedSessionIds())
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBaseOption[]>([])
  const [notebookFolders, setNotebookFolders] = useState<string[]>([])
  const [notebookEntryPaths, setNotebookEntryPaths] = useState<string[]>([])
  const [notebookVault, setNotebookVault] = useState<NotebookVaultInfo | null>(null)
  const [notebookVaults, setNotebookVaults] = useState<NotebookVaultInfo[]>([])
  const [selectedKnowledgeBaseIds, setSelectedKnowledgeBaseIds] = useState<string[]>([])
  const [selectedNotebookVaultId, setSelectedNotebookVaultId] = useState<string | null>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [traceCollapsed, setTraceCollapsed] = useState(true)
  const [noteFocusTarget, setNoteFocusTarget] = useState<Extract<SourceTarget, { type: 'notebook' }> | null>(null)
  const [knowledgeFocusTarget, setKnowledgeFocusTarget] = useState<Extract<SourceTarget, { type: 'kb' }> | null>(null)
  const [latestUsage, setLatestUsage] = useState<TokenUsagePayload | null>(null)
  const contextStats = useMemo<ContextStats>(() => {
    const config = activeLlmConfig(llmSettings)
    const providerInputTokens = typeof latestUsage?.input_tokens === 'number' ? latestUsage.input_tokens : null
    return {
      usedTokens: providerInputTokens ?? estimateContextTokens(messages, streamingText),
      maxTokens: config?.contextWindowTokens || DEFAULT_CONTEXT_WINDOW_TOKENS,
      source: providerInputTokens === null ? 'estimate' : 'provider',
    }
  }, [latestUsage, llmSettings, messages, streamingText])

  const pushStatus = useCallback((status: AgentStatus) => {
    if (status.kind === 'idle') return

    const text = status.detail ? `${status.label}: ${status.detail}` : status.label
    setMessages((prev) => {
      const last = prev[prev.length - 1]
      const transient = status.kind === 'thinking' || status.kind === 'tool'
      if (last?.role === 'status' && last.text === text && last.kind === status.kind) {
        return prev
      }
      if (
        last?.role === 'status' &&
        last.transient &&
        transient
      ) {
        return [
          ...prev.slice(0, -1),
          { role: 'status', text, kind: status.kind, transient },
        ]
      }
      return [...prev, { role: 'status', text, kind: status.kind, transient }]
    })
  }, [])

  const pushProgressContent = useCallback((text: string) => {
    if (!text.trim()) return
    setMessages((prev) => [
      ...dropTrailingTransientStatus(prev),
      { role: 'status', text, kind: 'thinking', transient: true },
    ])
  }, [])

  const hydrateSession = useCallback(async (id: string, settledHandoff = false) => {
    const selectionVersion = sessionSelectionVersionRef.current
    const hydrationVersion = ++sessionHydrationVersionRef.current
    const isCurrentHydration = () => isLatestSessionHydration(
      selectionVersion,
      sessionSelectionVersionRef.current,
      hydrationVersion,
      sessionHydrationVersionRef.current,
      id,
      activeSessionIdRef.current,
    )

    try {
      const res = await fetch(`/api/sessions/${id}`)
      if (!res.ok) {
        throw new Error(`failed to load session: HTTP ${res.status}`)
      }
      const data = await res.json() as SessionDetailResponse
      if (!isCurrentHydration()) return
      const restoredTrace = restoreTraceEntries(data.trace ?? [], data.compact_summary ?? null)
      const withCitations = attachRestoredCitations(
        (data.messages ?? []).map((message) => ({
          role: message.role,
          text: message.text,
          citations: message.citations,
        })),
        restoredTrace,
      )
      const restored = attachRestoredDeepSolve(withCitations, restoredTrace)
      setMessages((live) => reconcileSessionMessages(restored, live))
      setTraceEntries(restoredTrace)
      setLatestUsage(data.latest_usage ?? null)
      const restoredModelConfig = data.llm?.model
        ? llmSettings.llmConfigs.find((config) => config.model === data.llm?.model)
        : null
      setSelectedLlmConfigId(restoredModelConfig?.id ?? llmSettings.activeLlmConfigId)
      if (data.active_run && !settledHandoff) {
        setRunning(true)
        pushStatus({
          kind: 'thinking',
          label: 'Working',
          detail: [
            `Rejoining ${data.active_run.capability ? capabilityLabel(data.active_run.capability) : 'agent'} run`,
            data.active_run.current_stage ? `stage: ${data.active_run.current_stage}` : '',
          ].filter(Boolean).join(' · '),
        })
      } else if (data.run_state && ['interrupted', 'failed', 'cancelled'].includes(data.run_state.status ?? '')) {
        pushStatus({
          kind: data.run_state.status === 'cancelled' ? 'done' : 'error',
          label: data.run_state.status === 'interrupted' ? 'Run interrupted' : `Run ${data.run_state.status}`,
          detail: [
            data.run_state.capability ? capabilityLabel(data.run_state.capability) : 'Agent',
            data.run_state.current_stage ? `stage: ${data.run_state.current_stage}` : '',
          ].filter(Boolean).join(' · '),
        })
      }
      if (data.capability && isCapability(data.capability)) {
        const restoredCapability = data.capability === 'deep_solve' ? 'chat' : data.capability
        setCapability(restoredCapability)
        if (data.capability === 'deep_solve') {
          void fetch(`/api/sessions/${id}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ capability: 'chat' }),
          })
        }
      }
      setSelectedKnowledgeBaseIds(data.kbs ?? (data.kb ? [data.kb] : []))
      setSelectedNotebookVaultId(data.notebook_enabled
        ? (data.notebook_vault_id ?? notebookVaults.find((vault) => vault.active)?.id ?? null)
        : null)
      setTemporaryConversation(Boolean(data.temporary))
      const title = data.metadata?.name || restored.find((message) => message.role === 'user')?.text
      if (title) {
        updateRecentSessionTitle(setRecentSessions, id, sessionTitleFromMessage(title))
      }
    } catch (err) {
      if (!isCurrentHydration()) return
      const message = err instanceof Error ? err.message : String(err)
      setMessages((live) => reconcileSessionMessages(
        [{ role: 'assistant', text: `Error: ${message}` }],
        live,
      ))
    }
  }, [llmSettings, notebookVaults, pushStatus])

  const { send } = useWebSocket(sessionId, {
    onEvent: (event, sourceSessionId) => {
      if (!isCurrentSessionEvent(sourceSessionId, activeSessionIdRef.current)) {
        if (event.type === 'status') {
          const payload = event.payload as Record<string, unknown>
          const kind = payload.kind as string
          if (kind === 'running' || kind === 'stopping') {
            updateRecentSessionRun(setRecentSessions, sourceSessionId, runSummaryFromStatusPayload(payload))
          } else if (kind === 'done' || kind === 'stopped' || kind === 'error') {
            updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
          }
        }
        return
      }
      if (event.type === 'content') {
        if (event.payload.chunk) {
          streamingRef.current += event.payload.text
          setStreamingText(streamingRef.current)
        } else {
          const finalText = completedStreamText(streamingRef.current, event.payload.text)
          const citations = pendingCitationsRef.current
          const deepSolve = pendingDeepSolveRef.current
          const notebookEditProposal = pendingNotebookEditProposalRef.current
          if (finalText.trim() || citations.length > 0 || deepSolve.length > 0 || notebookEditProposal) {
            setMessages((prev) => appendCompletedSessionMessage(
              dropTrailingTransientStatus(prev),
              {
                role: 'assistant',
                text: finalText,
                citations,
                deepSolve: deepSolve.length > 0 ? deepSolve : undefined,
                notebookEditProposal,
              },
            ))
          } else {
            setMessages((prev) => dropTrailingTransientStatus(prev))
          }
          pendingCitationsRef.current = []
          pendingDeepSolveRef.current = []
          pendingNotebookEditProposalRef.current = undefined
          streamingRef.current = ''
          progressStreamingRef.current = ''
          setStreamingText('')
          setRunning(false)
          if (citations.length > 0) {
            void persistMessageCitations(sourceSessionId, citations).catch((err) => {
              console.warn('failed to persist message citations', err)
            })
          }
          void refreshSessions()
        }
      } else if (event.type === 'progress_content') {
        if (event.payload.chunk) {
          progressStreamingRef.current += event.payload.text
        } else {
          progressStreamingRef.current = event.payload.text
        }
        pushProgressContent(progressStreamingRef.current)
      } else if (event.type === 'trace') {
        const runtimeUsage = tokenUsageFromRuntimeTrace(event.payload as Record<string, unknown>)
        if (runtimeUsage) {
          setLatestUsage(runtimeUsage)
        }
        const citations = citationsFromTrace(event.payload as Record<string, unknown>)
        if (citations.length > 0) {
          pendingCitationsRef.current = mergeCitations(pendingCitationsRef.current, citations)
        }
        const deepSolveEvent = deepSolveEventFromTrace(event.payload as Record<string, unknown>)
        if (deepSolveEvent) {
          pendingDeepSolveRef.current = [...pendingDeepSolveRef.current, deepSolveEvent]
        }
        const notebookEditProposal = notebookEditProposalFromTrace(event.payload as Record<string, unknown>)
        if (notebookEditProposal) {
          pendingNotebookEditProposalRef.current = notebookEditProposal
        }
        pushStatus(statusFromTrace(event.payload as Record<string, unknown>))
        setTraceEntries((prev) => [
          ...prev,
          { kind: event.payload.kind, payload: event.payload, timestamp: Date.now() },
        ])
      } else if (event.type === 'status') {
        const payload = event.payload as Record<string, unknown>
        const kind = payload.kind as string
        if (kind === 'approval_request') {
          const requestId = typeof payload.request_id === 'string' ? payload.request_id : ''
          const tool = payload.tool === 'memory_forget' ? 'memory_forget' : 'memory_write'
          if (requestId) {
            setMemoryApproval({
              requestId,
              sessionId: sourceSessionId,
              tool,
              args: payload.args && typeof payload.args === 'object'
                ? payload.args as Record<string, unknown>
                : {},
            })
          }
        } else if (kind === 'approval_response_received' || kind === 'approval_response_rejected') {
          setMemoryApproval((current) => current?.requestId === payload.request_id ? null : current)
        } else if (kind === 'running') {
          setRunning(true)
          updateRecentSessionRun(setRecentSessions, sourceSessionId, runSummaryFromStatusPayload(payload))
          pushStatus({
            kind: 'thinking',
            label: 'Working',
            detail: payload.rejoined === true
              ? `Rejoined ${typeof payload.capability === 'string' ? capabilityLabel(payload.capability) : 'agent'} run`
              : typeof payload.capability === 'string' ? capabilityLabel(payload.capability) : undefined,
          })
        } else if (kind === 'done') {
          setMemoryApproval(null)
          setRunning(false)
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
          setLatestUsage((prev) => isTokenUsagePayload(payload.usage) ? payload.usage : prev)
          setMessages((prev) => dropTrailingTransientStatus(prev))
        } else if (kind === 'history_sync') {
          streamingRef.current = ''
          progressStreamingRef.current = ''
          setStreamingText('')
          setRunning(false)
          setMessages((prev) => dropTrailingTransientStatus(prev))
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
          void hydrateSession(sourceSessionId, true)
        } else if (kind === 'stopped') {
          setMemoryApproval(null)
          progressStreamingRef.current = ''
          pushStatus({
            kind: 'done',
            label: 'Stopped',
            detail: typeof payload.capability === 'string' ? capabilityLabel(payload.capability) : undefined,
          })
          setRunning(false)
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
        } else if (kind === 'stopping') {
          updateRecentSessionRun(setRecentSessions, sourceSessionId, runSummaryFromStatusPayload({ ...payload, status: 'cancelling' }))
          pushStatus({
            kind: 'thinking',
            label: 'Stopping',
            detail: typeof payload.capability === 'string' ? capabilityLabel(payload.capability) : undefined,
          })
        } else if (kind === 'context_repaired') {
          pushStatus({
            kind: 'tool',
            label: 'Context repaired',
            detail: payload.reason === 'incomplete_tool_call' ? 'Recovered incomplete tool call history' : undefined,
          })
        } else if (kind === 'error') {
          setMemoryApproval(null)
          progressStreamingRef.current = ''
          const message = typeof payload.message === 'string' ? payload.message : 'WebSocket error'
          pushStatus({ kind: 'error', label: 'Error', detail: message })
          setRunning(false)
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
        }
      }
    },
    onClose: (sourceSessionId) => {
      if (!isCurrentSessionEvent(sourceSessionId, activeSessionIdRef.current)) return
      setRunning(false)
      pushStatus({ kind: 'idle', label: 'Disconnected', detail: 'WebSocket closed' })
    },
    onError: (sourceSessionId) => {
      if (!isCurrentSessionEvent(sourceSessionId, activeSessionIdRef.current)) return
      pushStatus({ kind: 'error', label: 'Connection failed', detail: 'Check the Folumi backend service' })
      setMessages((prev) => [
        ...prev,
        { role: 'assistant', text: 'Error: WebSocket connection failed. Check that the Folumi backend is running on 127.0.0.1:8080.' },
      ])
      setRunning(false)
    },
  })

  const refreshSessions = useCallback(async () => {
    const res = await fetch('/api/sessions')
    if (!res.ok) {
      throw new Error(`failed to load sessions: HTTP ${res.status}`)
    }
    const data = await res.json() as SessionListResponse
    setRecentSessions(sortRecentSessions((data.sessions ?? []).map((session) => ({
      id: session.id,
      title: session.title || session.name || 'New session',
      activeRun: session.active_run ?? null,
      pinned: pinnedSessionIds.has(session.id),
      temporary: Boolean(session.temporary),
    }))))
  }, [pinnedSessionIds])

  const reconcileActiveSessionRuns = useCallback(async () => {
    const res = await fetch('/api/sessions')
    if (!res.ok) {
      throw new Error(`failed to reconcile session runs: HTTP ${res.status}`)
    }
    const data = await res.json() as SessionListResponse
    const incoming = (data.sessions ?? []).map((session) => ({
      id: session.id,
      activeRun: session.active_run ?? null,
    }))
    setRecentSessions((current) => reconcileSessionRunState(current, incoming))
    const currentSessionId = activeSessionIdRef.current
    if (currentSessionId && incoming.some((session) => session.id === currentSessionId && !session.activeRun)) {
      setRunning(false)
    }
  }, [])

  const hasTrackedActiveRuns = recentSessions.some((session) => Boolean(session.activeRun))
  useEffect(() => {
    if (!hasTrackedActiveRuns) return
    const timer = window.setInterval(() => {
      void reconcileActiveSessionRuns().catch((err) => {
        console.warn('failed to reconcile background session runs', err)
      })
    }, 1500)
    return () => window.clearInterval(timer)
  }, [hasTrackedActiveRuns, reconcileActiveSessionRuns])

  const refreshKnowledgeBases = useCallback(async () => {
    const res = await fetch('/api/knowledge-bases')
    if (!res.ok) {
      throw new Error(`failed to load knowledge bases: HTTP ${res.status}`)
    }
    const data = await res.json() as { knowledge_bases?: KnowledgeBaseOption[] }
    const items = data.knowledge_bases ?? []
    setKnowledgeBases(items.map((item) => ({ id: item.id, name: item.name })))
    setSelectedKnowledgeBaseIds((current) => current.filter((id) => items.some((item) => item.id === id)))
  }, [])

  const refreshNotebookFolders = useCallback(async () => {
    const res = await fetch('/api/notebook/entries?space_id=default')
    if (!res.ok) {
      throw new Error(`failed to load notebook folders: HTTP ${res.status}`)
    }
    const data = await safeJson(res)
    setNotebookFolders(((data.folders ?? []) as string[]).filter(Boolean))
    setNotebookEntryPaths(((data.entries ?? []) as Array<{ path?: string | null }>)
      .map((entry) => entry.path ?? '')
      .filter(Boolean))
    setNotebookVault((data.vault ?? null) as NotebookVaultInfo | null)
    setNotebookVaults((data.vaults ?? []) as NotebookVaultInfo[])
  }, [])

  useEffect(() => {
    const pending = pendingSessionSendRef.current
    if (!pending || pending.sessionId !== sessionId) return
    pendingSessionSendRef.current = null
    send({ type: 'message', content: pending.content })
  }, [sessionId, send])

  const persistSettings = useCallback((nextSettings: typeof llmSettings) => {
    saveLlmSettings(nextSettings)
    saveStoredLlmSettings(nextSettings).catch((err) => {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Settings not saved', detail: message })
    })
  }, [pushStatus])

  useEffect(() => {
    let cancelled = false
    loadStoredLlmSettings()
      .then((storedSettings) => {
        if (cancelled) return
        if (storedSettings) {
          setLlmSettings(storedSettings)
          setSelectedLlmConfigId(storedSettings.activeLlmConfigId)
          saveLlmSettings(storedSettings)
        } else if (hasLocalLlmSettings()) {
          const localSettings = loadLlmSettings()
          void saveStoredLlmSettings(localSettings)
        }
      })
      .catch((err) => {
        if (cancelled) return
        const message = err instanceof Error ? err.message : String(err)
        pushStatus({ kind: 'error', label: 'Settings load failed', detail: message })
      })
      .finally(() => {
        if (!cancelled) setSettingsHydrated(true)
      })
    return () => {
      cancelled = true
    }
  }, [pushStatus])

  useEffect(() => {
    if (!settingsHydrated) return
    if (shouldShowOnboarding(llmSettings)) {
      setOnboardingOpen(true)
    }
  }, [llmSettings.onboardingCompleted, llmSettings.onboardingVersion, settingsHydrated])

  useEffect(() => {
    void setNativeWindowTheme(llmSettings.theme).catch((error) => {
      console.warn('failed to update native window theme', error)
    })
  }, [llmSettings.theme])

  useEffect(() => {
    refreshSessions().catch((err) => {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
    })
    refreshKnowledgeBases().catch((err) => {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
    })
    refreshNotebookFolders().catch((err) => {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
    })
  }, [refreshSessions, refreshKnowledgeBases, refreshNotebookFolders, pushStatus])

  useEffect(() => {
    const refresh = () => {
      void refreshNotebookFolders().catch((err) => {
        const message = err instanceof Error ? err.message : String(err)
        pushStatus({ kind: 'error', label: 'Error', detail: message })
      })
    }
    window.addEventListener('folumi:notebook-vaults-changed', refresh)
    return () => window.removeEventListener('folumi:notebook-vaults-changed', refresh)
  }, [pushStatus, refreshNotebookFolders])

  const handleSend = useCallback(async (text: string, attachments: ChatAttachment[] = []) => {
    try {
      const content = buildMessageContentWithAttachments(text, attachments)
      const displayText = text.trim() || `Sent ${attachments.length} attachment(s)`
      let sid = sessionId
      let createdSession = false
      if (!sid) {
        const res = await fetch('/api/sessions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            capability,
            kbs: selectedKnowledgeBaseIds,
            notebook_enabled: Boolean(selectedNotebookVaultId),
            notebook_vault_id: selectedNotebookVaultId,
            temporary: temporaryConversation,
            assistant: {
              name: llmSettings.assistantName,
              instructions: llmSettings.assistantInstructions,
            },
            llm: settingsForSession(llmSettings, selectedLlmConfigId),
            search: searchForSession(llmSettings),
          }),
        })
        if (!res.ok) {
          throw new Error(`failed to create session: HTTP ${res.status}`)
        }
        const data = await res.json()
        const createdSessionId = data.id as string
        sid = createdSessionId
        createdSession = true
        pendingSessionSendRef.current = { sessionId: createdSessionId, content }
        activateSession(createdSessionId)
        promoteRecentSession(setRecentSessions, createdSessionId, sessionTitleFromMessage(displayText), temporaryConversation)
      } else {
        promoteRecentSession(setRecentSessions, sid, sessionTitleFromMessage(displayText))
      }

      setMessages((prev) => [...prev, { role: 'user', text: displayText, attachments }])
      setRunning(true)
      pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
      if (!createdSession) send({ type: 'message', content }, sid)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
      setRunning(false)
    }
  }, [sessionId, capability, llmSettings, selectedLlmConfigId, selectedKnowledgeBaseIds, selectedNotebookVaultId, temporaryConversation, send, pushStatus, activateSession])

  const handleStopGeneration = useCallback(() => {
    if (!running) return
    send({ type: 'stop' })
    pushStatus({ kind: 'tool', label: 'Stopping', detail: capabilityLabel(capability) })
  }, [capability, pushStatus, running, send])

  const handleEditUserMessage = useCallback(async (messageIndex: number, nextText: string) => {
    if (running || !nextText.trim()) return
    try {
      const priorMessages = messages
        .slice(0, messageIndex)
        .filter((message) => message.role === 'user' || message.role === 'assistant')
      if (sessionId) {
        const forkRes = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}/fork-before-message`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            message_index: messageIndex,
            label: 'edited user message',
          }),
        })
        const forkData = await safeJson(forkRes)
        if (!forkRes.ok) {
          throw new Error(errorMessage(forkData, forkRes.status))
        }
        if (forkData.forked === true) {
          setMessages([...priorMessages, { role: 'user', text: nextText }])
          setTraceEntries([])
          setLatestUsage(null)
          setStreamingText('')
          streamingRef.current = ''
          progressStreamingRef.current = ''
          pendingCitationsRef.current = []
          pendingDeepSolveRef.current = []
          pendingNotebookEditProposalRef.current = undefined
          setRunning(true)
          pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
          send({ type: 'message', content: nextText })
          promoteRecentSession(setRecentSessions, sessionId, sessionTitleFromMessage(nextText))
          return
        }
      }

      const res = await fetch('/api/sessions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          capability,
          kbs: selectedKnowledgeBaseIds,
          notebook_enabled: Boolean(selectedNotebookVaultId),
          notebook_vault_id: selectedNotebookVaultId,
          temporary: temporaryConversation,
          assistant: {
            name: llmSettings.assistantName,
            instructions: llmSettings.assistantInstructions,
          },
          llm: settingsForSession(llmSettings, selectedLlmConfigId),
          search: searchForSession(llmSettings),
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) {
        throw new Error(errorMessage(data, res.status))
      }
      const nextSessionId = data.id as string

      activateSession(nextSessionId)
      promoteRecentSession(setRecentSessions, nextSessionId, sessionTitleFromMessage(nextText), temporaryConversation)
      setMessages([{ role: 'user', text: nextText }])
      setTraceEntries([])
      setLatestUsage(null)
      setStreamingText('')
      streamingRef.current = ''
      progressStreamingRef.current = ''
      pendingCitationsRef.current = []
      pendingDeepSolveRef.current = []
      pendingNotebookEditProposalRef.current = undefined
      setRunning(true)
      pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
      pendingSessionSendRef.current = { sessionId: nextSessionId, content: nextText }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
      setRunning(false)
    }
  }, [capability, llmSettings, selectedLlmConfigId, messages, pushStatus, running, selectedKnowledgeBaseIds, selectedNotebookVaultId, temporaryConversation, send, sessionId, activateSession])

  const handleSaveToNotebook = useCallback(async (markdown: string, options: SaveToNotebookOptions = {}): Promise<SaveToNotebookResult> => {
    try {
      const title = options.title?.trim() || titleFromMarkdown(markdown)
      const entryType = resolveGeneratedNotebookEntryType(options.entryType)
      let folderPath = options.newFolderPath?.trim() || options.folderPath?.trim() || ''
      if (folderPath) {
        folderPath = normalizeNotebookFolderPath(folderPath)
      }
      if (options.newFolderPath?.trim()) {
        const folderRes = await fetch('/api/notebook/folders', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: folderPath }),
        })
        const folderData = await safeJson(folderRes)
        if (!folderRes.ok) {
          throw new Error(errorMessage(folderData, folderRes.status))
        }
        setNotebookFolders(((folderData.folders ?? []) as string[]).filter(Boolean))
      }
      const path = options.filePath
        ? normalizeNotebookEntryPath(options.filePath)
        : folderPath
          ? `${folderPath}/${notebookFileNameFromTitle(title)}`
          : notebookFileNameFromTitle(title)
      if (!path) throw new Error('Notebook path is invalid')
      const res = await fetch('/api/notebook/entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          space_id: 'default',
          entry_type: entryType,
          title,
          path,
          markdown,
          metadata: {
            generatedBy: 'chat',
            generatedAt: new Date().toISOString(),
            sourceSessionId: sessionId,
          },
          source_session_id: sessionId,
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) {
        throw new Error(errorMessage(data, res.status))
      }
      if (!options.newFolderPath?.trim()) {
        void refreshNotebookFolders()
      }
      const entry = data.entry as { id: string; title: string; path?: string | null }
      const savedPath = entry.path ?? path
      setNotebookEntryPaths((current) => current.includes(savedPath) ? current : [...current, savedPath])
      pushStatus({ kind: 'done', label: 'Saved', detail: `Notebook: ${savedPath}` })
      return { entryId: entry.id, title: entry.title, path: savedPath }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Save failed', detail: message })
      throw err
    }
  }, [pushStatus, refreshNotebookFolders, sessionId])

  const handleApplyNotebookEdit = useCallback(async (proposal: NotebookEditProposal) => {
    try {
      const detailRes = await fetch(`/api/notebook/entries/${encodeURIComponent(proposal.entryId)}`)
      const detailData = await safeJson(detailRes)
      if (!detailRes.ok) {
        throw new Error(errorMessage(detailData, detailRes.status))
      }
      const res = await fetch(`/api/notebook/entries/${encodeURIComponent(proposal.entryId)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_revision: detailData.revision,
          path: (detailData.entry as { path?: string | null } | undefined)?.path,
          title: proposal.proposedTitle,
          markdown: proposal.proposedMarkdown,
          metadata: {
            updated_by: 'agent_proposal',
            proposal_kind: proposal.proposalKind ?? 'edit',
            proposal_summary: proposal.summary,
            suggested_links: proposal.suggestedLinks ?? [],
            suggested_tags: proposal.suggestedTags ?? [],
            merge_source_entry_ids: proposal.mergeSourceEntryIds ?? [],
            source_session_id: sessionId,
          },
        }),
      })
      const data = await safeJson(res)
      if (!res.ok) {
        throw new Error(errorMessage(data, res.status))
      }
      setMessages((prev) => prev.map((message) => {
        if (message.notebookEditProposal?.entryId !== proposal.entryId) return message
        return {
          ...message,
          notebookEditProposal: {
            ...message.notebookEditProposal,
            applied: true,
          },
        }
      }))
      pushStatus({ kind: 'done', label: 'Notebook updated', detail: proposal.proposedTitle })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Notebook update failed', detail: message })
    }
  }, [pushStatus, sessionId])

  const handleAskDeepSolveStep = useCallback((step: { id: string; title: string; summary?: string }) => {
    const prompt = [
      `Please explain Deep Solve step ${step.id}: ${step.title}.`,
      step.summary ? `Step summary: ${step.summary}` : '',
      'Focus only on this step, clarify why it works, and connect it back to the original problem.',
    ]
      .filter(Boolean)
      .join('\n\n')
    void handleSend(prompt)
  }, [handleSend])

  const handleSettingsChange = (nextSettings: typeof llmSettings) => {
    setLlmSettings(nextSettings)
    setSelectedLlmConfigId(nextSettings.activeLlmConfigId)
    persistSettings(nextSettings)
    if (settingsRequireSessionReset(llmSettings, nextSettings)) {
      activateSession(null)
    }
  }

  const startNewChat = useCallback(() => {
    activateSession(null)
    setTemporaryConversation(false)
    setCapability('chat')
    setSelectedLlmConfigId(llmSettings.activeLlmConfigId)
    setMessages([])
    setStreamingText('')
    streamingRef.current = ''
    progressStreamingRef.current = ''
    setTraceEntries([])
    pendingCitationsRef.current = []
    pendingDeepSolveRef.current = []
    pendingNotebookEditProposalRef.current = undefined
    setLatestUsage(null)
    setRunning(false)
    setView('assistant')
  }, [activateSession, llmSettings.activeLlmConfigId])

  const handleNavigate = useCallback((nextView: AppView) => {
    if (nextView === 'assistant') {
      startNewChat()
      return
    }
    if (nextView === 'settings') setSettingsTab('llm')
    setView(nextView)
  }, [startNewChat])

  const completeOnboarding = useCallback(() => {
    const nextSettings = completeOnboardingSettings(llmSettings, CURRENT_ONBOARDING_VERSION)
    setLlmSettings(nextSettings)
    persistSettings(nextSettings)
    setOnboardingStep(0)
    setOnboardingOpen(false)
  }, [llmSettings, persistSettings])

  const openOnboarding = useCallback(() => {
    if (!shouldShowOnboarding(llmSettings)) setOnboardingStep(0)
    setOnboardingOpen(true)
  }, [llmSettings])

  const pauseOnboarding = useCallback(() => {
    setOnboardingOpen(false)
  }, [])

  const startGuideAssistant = useCallback(() => {
    startNewChat()
    setCapability('chat')
    setSelectedKnowledgeBaseIds([])
    setSelectedNotebookVaultId(null)
    setStarterDraft({ id: Date.now(), text: guideAssistantStarterPrompt(llmSettings.language) })
  }, [llmSettings.language, startNewChat])

  const handleGuideNavigate = useCallback((destination: ProductGuideDestination) => {
    if (destination === 'chat') {
      startNewChat()
      return
    }
    if (destination === 'embedding-settings' || destination === 'notebook-settings') {
      setSettingsTab(destination === 'embedding-settings' ? 'embedding' : 'notebook')
      setView('settings')
      return
    }
    if (destination === 'memory') {
      setView('memory')
      return
    }
    setView(destination === 'notebook' ? 'notebook' : 'knowledge')
  }, [startNewChat])

  const handleKnowledgeBaseToggle = useCallback(async (knowledgeBaseId: string) => {
    if (running) return
    const nextKnowledgeBaseIds = selectedKnowledgeBaseIds.includes(knowledgeBaseId)
      ? selectedKnowledgeBaseIds.filter((id) => id !== knowledgeBaseId)
      : [...selectedKnowledgeBaseIds, knowledgeBaseId]
    setSelectedKnowledgeBaseIds(nextKnowledgeBaseIds)
    if (!sessionId) return

    try {
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ kbs: nextKnowledgeBaseIds }),
      })
      if (!res.ok) {
        throw new Error(`failed to update session knowledge base: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [running, selectedKnowledgeBaseIds, sessionId])

  const handleNotebookVaultToggle = useCallback(async (vaultId: string) => {
    if (running) return
    const nextVaultId = selectedNotebookVaultId === vaultId ? null : vaultId
    setSelectedNotebookVaultId(nextVaultId)
    if (!sessionId) return

    try {
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(nextVaultId
          ? { notebook_enabled: true, notebook_vault_id: nextVaultId }
          : { notebook_enabled: false }),
      })
      if (!res.ok) {
        throw new Error(`failed to update session source: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [running, selectedNotebookVaultId, sessionId])

  const handleLlmConfigChange = useCallback(async (id: string) => {
    if (running) return
    const nextSettings = { ...llmSettings, activeLlmConfigId: id }
    setLlmSettings(nextSettings)
    setSelectedLlmConfigId(id)
    persistSettings(nextSettings)
    if (!sessionId) return

    try {
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ llm: settingsForSession(nextSettings) }),
      })
      if (!res.ok) {
        throw new Error(`failed to update session model: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [llmSettings, persistSettings, running, sessionId])

  const handleSelectSession = async (id: string) => {
    if (id !== sessionId) {
      activateSession(id)
      setMessages([])
      setStreamingText('')
      streamingRef.current = ''
      progressStreamingRef.current = ''
      setTraceEntries([])
      pendingCitationsRef.current = []
      pendingDeepSolveRef.current = []
      pendingNotebookEditProposalRef.current = undefined
      setLatestUsage(null)
      setRunning(false)
      await hydrateSession(id)
    }
    setView('assistant')
  }

  const handleRenameSession = async (id: string, title: string) => {
    const previousSessions = recentSessions
    setRecentSessions((prev) =>
      prev.map((session) => (session.id === id ? { ...session, title } : session)),
    )

    try {
      const res = await fetch(`/api/sessions/${id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: title }),
      })
      if (!res.ok) {
        throw new Error(`failed to rename session: HTTP ${res.status}`)
      }
      void refreshSessions()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setRecentSessions(previousSessions)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }

  const handleTogglePinSession = useCallback((id: string) => {
    setPinnedSessionIds((current) => {
      const next = new Set(current)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      savePinnedSessionIds(next)
      setRecentSessions((sessions) =>
        sortRecentSessions(sessions.map((session) =>
          session.id === id ? { ...session, pinned: next.has(id) } : session,
        )),
      )
      return next
    })
  }, [])

  const handleDeleteSession = async (id: string) => {
    const session = recentSessions.find((item) => item.id === id)
    if (!window.confirm(`Delete "${session?.title ?? 'this session'}"?`)) return

    const previousSessions = recentSessions
    setRecentSessions((prev) => prev.filter((item) => item.id !== id))
    if (sessionId === id) {
      activateSession(null)
      setMessages([])
      setStreamingText('')
      streamingRef.current = ''
      progressStreamingRef.current = ''
      setTraceEntries([])
      pendingCitationsRef.current = []
      pendingDeepSolveRef.current = []
      pendingNotebookEditProposalRef.current = undefined
      setLatestUsage(null)
    }

    try {
      const res = await fetch(`/api/sessions/${id}`, { method: 'DELETE' })
      if (!res.ok) {
        throw new Error(`failed to delete session: HTTP ${res.status}`)
      }
      setPinnedSessionIds((current) => {
        if (!current.has(id)) return current
        const next = new Set(current)
        next.delete(id)
        savePinnedSessionIds(next)
        return next
      })
      void refreshSessions()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setRecentSessions(previousSessions)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }

  const handleSourceNavigate = useCallback((target: SourceTarget, reference: SourceReference) => {
    if (target.type === 'chat') {
      void handleSelectSession(target.sessionId)
      pushStatus({
        kind: 'done',
        label: 'Opened source',
        detail: target.messageId ? `Chat message ${target.messageId}` : 'Chat session',
      })
      return
    }

    if (target.type === 'web') {
      void openExternalUrl(target.url)
        .then((opened) => {
          if (!opened) window.open(target.url, '_blank', 'noopener,noreferrer')
        })
        .catch(() => {
          window.open(target.url, '_blank', 'noopener,noreferrer')
        })
      return
    }

    if (target.type === 'notebook') {
      setNoteFocusTarget(target)
      setView('notebook')
      pushStatus({
        kind: 'done',
        label: 'Opened source area',
        detail: sourceTargetDetail(target, reference),
      })
      return
    }

    if (target.type === 'kb') {
      setKnowledgeFocusTarget(target)
      setView('knowledge')
      pushStatus({
        kind: 'done',
        label: 'Opened source area',
        detail: sourceTargetDetail(target, reference),
      })
    }
  }, [handleSelectSession, pushStatus])

  const chatIsEmpty = view === 'assistant' && messages.length === 0 && !streamingText && !sessionId
  const t = (key: TranslationKey) => translate(llmSettings.language, key)

  return (
    <I18nProvider language={llmSettings.language}>
    <div className="app-shell flex h-screen flex-col overflow-hidden" data-theme={llmSettings.theme}>
      <AppTitleBar
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
      />
      <div className="flex min-h-0 flex-1 overflow-hidden">
      <Sidebar
        activeView={view}
        activeSessionId={view === 'assistant' ? sessionId : null}
        collapsed={sidebarCollapsed}
        recentSessions={recentSessions}
        onNavigate={handleNavigate}
        onSelectSession={handleSelectSession}
        onRenameSession={handleRenameSession}
        onDeleteSession={handleDeleteSession}
        onTogglePinSession={handleTogglePinSession}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {view === 'assistant' && (
          <>
            <header className="flex items-center gap-5 border-b border-gray-100 bg-white px-6 py-3.5">
              <div className="min-w-0">
                <h1 className="text-lg font-semibold text-gray-900">{llmSettings.assistantName || t('chat.title')}</h1>
                <p className="text-xs text-gray-500">{t('chat.subtitle')}</p>
              </div>
              <div className="ml-auto">
                <label
                  className={`flex items-center gap-2.5 text-xs font-medium transition-colors ${
                    temporaryConversation ? 'text-amber-800' : 'text-gray-600'
                  } ${sessionId || running ? 'cursor-default opacity-60' : 'cursor-pointer'}`}
                  title={llmSettings.language === 'en-US'
                    ? 'Does not use personal information or past conversations, and cannot be found again later.'
                    : '不使用个人信息或过往对话，结束后也不会被再次查找。'}
                >
                  <span>{llmSettings.language === 'en-US' ? 'Temporary chat' : '临时对话'}</span>
                  <input
                    type="checkbox"
                    className="peer sr-only"
                    checked={temporaryConversation}
                    disabled={Boolean(sessionId) || running}
                    onChange={(event) => setTemporaryConversation(event.target.checked)}
                  />
                  <span className="relative h-5 w-9 rounded-full bg-gray-300 transition peer-checked:bg-amber-500 peer-focus-visible:ring-2 peer-focus-visible:ring-amber-200 peer-disabled:opacity-70 after:absolute after:left-0.5 after:top-0.5 after:h-4 after:w-4 after:rounded-full after:bg-white after:shadow-sm after:transition peer-checked:after:translate-x-4" />
                </label>
              </div>
            </header>
            <div className="flex min-h-0 flex-1 overflow-hidden">
              <main className="min-h-0 min-w-0 flex-1 overflow-hidden">
                <ChatBox
                  sessionId={sessionId}
                  messages={messages}
                  streamingText={streamingText}
                  contextStats={contextStats}
                  llmConfigs={llmSettings.llmConfigs}
                  activeLlmConfigId={selectedLlmConfigId}
                  knowledgeBases={knowledgeBases}
                  selectedKnowledgeBaseIds={selectedKnowledgeBaseIds}
                  notebookVaults={notebookVaults}
                  selectedNotebookVaultId={selectedNotebookVaultId}
                  initialDraft={starterDraft}
                  onSend={handleSend}
                  onStop={handleStopGeneration}
                  onEditUserMessage={handleEditUserMessage}
                  onAskDeepSolveStep={handleAskDeepSolveStep}
                  onKnowledgeBaseToggle={handleKnowledgeBaseToggle}
                  onNotebookVaultToggle={handleNotebookVaultToggle}
                  onLlmConfigChange={handleLlmConfigChange}
                  notebookFolders={notebookFolders}
                  notebookEntryPaths={notebookEntryPaths}
                  notebookVault={notebookVault}
                  onSaveToNotebook={handleSaveToNotebook}
                  onOpenNotebookEntry={(entryId) => {
                    setNoteFocusTarget({ type: 'notebook', entryId })
                    setView('notebook')
                  }}
                  onApplyNotebookEdit={handleApplyNotebookEdit}
                  onSourceNavigate={handleSourceNavigate}
                  disabled={false}
                  running={running}
                />
              </main>
              {!chatIsEmpty && (
                <aside
                  className={`min-h-0 shrink-0 overflow-hidden bg-white transition-[width] duration-200 ${
                    traceCollapsed ? 'w-12' : 'w-72'
                  }`}
                >
                  <TracePanel
                    entries={traceEntries}
                    collapsed={traceCollapsed}
                    onToggleCollapsed={() => setTraceCollapsed((value) => !value)}
                  />
                </aside>
              )}
            </div>
          </>
        )}

        {view === 'knowledge' && (
          <KnowledgeBasePage
            settings={llmSettings}
            onChanged={refreshKnowledgeBases}
            focusTarget={knowledgeFocusTarget}
          />
        )}

        {view === 'notebook' && (
          <NotesPage
            language={llmSettings.language}
            focusTarget={noteFocusTarget}
            onSourceNavigate={handleSourceNavigate}
            onManageVaults={() => {
              setSettingsTab('notebook')
              setView('settings')
            }}
          />
        )}

        {view === 'memory' && (
          <UserMemoryPage
            language={llmSettings.language}
            assistantName={llmSettings.assistantName}
            assistantInstructions={llmSettings.assistantInstructions}
            onSessionNavigate={(sourceSessionId) => { void handleSelectSession(sourceSessionId) }}
            onAssistantProfileChange={({ name, instructions }) => handleSettingsChange({
              ...llmSettings,
              assistantName: name,
              assistantInstructions: instructions,
            })}
          />
        )}

        {view === 'settings' && (
          <SettingsPage
            settings={llmSettings}
            activeTab={settingsTab}
            onTabChange={setSettingsTab}
            onChange={handleSettingsChange}
            onOpenOnboarding={openOnboarding}
            onGuideNavigate={handleGuideNavigate}
            onStartGuideAssistant={startGuideAssistant}
          />
        )}
      </div>
      </div>

      {memoryApproval && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4" role="dialog" aria-modal="true" aria-labelledby="memory-approval-title">
          <div className="w-full max-w-lg rounded-xl border border-gray-200 bg-white p-5 shadow-2xl">
            <div className="flex items-start gap-3">
              <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700"><UserMemoryApprovalIcon /></span>
              <div>
                <h2 id="memory-approval-title" className="font-semibold text-gray-950">{llmSettings.language === 'en-US'
                  ? memoryApproval.tool === 'memory_write' ? 'Save this memory?' : 'Forget this memory?'
                  : memoryApproval.tool === 'memory_write' ? '保存这条个人信息吗？' : '移除这条个人信息吗？'}</h2>
                <p className="mt-1 text-sm leading-6 text-gray-500">{llmSettings.language === 'en-US'
                  ? 'The assistant cannot change long-term memory until you approve this exact operation.'
                  : '在你批准这次精确操作之前，助手无法更改你的个人信息。'}</p>
              </div>
            </div>
            <div className="mt-4 rounded-lg border border-gray-200 bg-gray-50 p-4 text-sm">
              {memoryApproval.tool === 'memory_write' ? (
                <>
                  <div className="mb-2 flex gap-2 text-xs text-gray-500"><span className="rounded-full bg-blue-50 px-2 py-0.5 text-blue-700">{String(memoryApproval.args.kind ?? 'preference')}</span></div>
                  <p className="whitespace-pre-wrap leading-6 text-gray-900">{String(memoryApproval.args.content ?? '')}</p>
                </>
              ) : (
                <p className="leading-6 text-red-700">{llmSettings.language === 'en-US' ? 'This removes the item body and recoverable revisions permanently.' : '这会永久移除条目正文及可恢复的历史版本。'}</p>
              )}
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button className="btn-secondary" type="button" onClick={() => {
                send({ type: 'approval_response', request_id: memoryApproval.requestId, approved: false }, memoryApproval.sessionId)
                setMemoryApproval(null)
              }}>{llmSettings.language === 'en-US' ? 'Deny' : '拒绝'}</button>
              <button className={memoryApproval.tool === 'memory_forget' ? 'inline-flex h-9 items-center rounded-md bg-red-600 px-4 text-sm font-medium text-white hover:bg-red-700' : 'btn-primary'} type="button" onClick={() => {
                send({ type: 'approval_response', request_id: memoryApproval.requestId, approved: true }, memoryApproval.sessionId)
                setMemoryApproval(null)
              }}>{llmSettings.language === 'en-US' ? 'Approve' : '批准'}</button>
            </div>
          </div>
        </div>
      )}

      {settingsHydrated && shouldShowOnboarding(llmSettings) && !onboardingOpen && (
        <OnboardingResumeButton onClick={openOnboarding} />
      )}
      {onboardingOpen && (
        <OnboardingDialog
          settings={llmSettings}
          knowledgeBaseCount={knowledgeBases.length}
          step={onboardingStep}
          onStepChange={setOnboardingStep}
          onOpenModelSettings={() => {
            setOnboardingOpen(false)
            setSettingsTab('llm')
            setView('settings')
          }}
          onOpenEmbeddingSettings={() => {
            setOnboardingOpen(false)
            setSettingsTab('embedding')
            setView('settings')
          }}
          onOpenKnowledge={() => {
            setOnboardingOpen(false)
            setView('knowledge')
          }}
          onOpenNotebook={() => {
            setOnboardingOpen(false)
            setView('notebook')
          }}
          onDismiss={pauseOnboarding}
          onComplete={completeOnboarding}
          onStart={() => {
            completeOnboarding()
            startNewChat()
            setStarterDraft({
              id: Date.now(),
              text: llmSettings.language === 'en-US'
                ? 'Summarize the three key conclusions in my material and cite where each one came from.'
                : '请总结我资料中的三个关键结论，并标明每个结论分别来自哪里。',
            })
          }}
        />
      )}
    </div>
    </I18nProvider>
  )
}

function UserMemoryApprovalIcon() {
  return <span className="text-lg leading-none">✦</span>
}

function dropTrailingTransientStatus(messages: Message[]) {
  const last = messages[messages.length - 1]
  if (last?.role === 'status' && last.transient) {
    return messages.slice(0, -1)
  }
  return messages
}

function statusFromTrace(payload: Record<string, unknown>): AgentStatus {
  const kind = payload.kind
  const capability = typeof payload.capability === 'string' ? capabilityLabel(payload.capability) : 'Agent'
  const phase = typeof payload.phase === 'string' ? phaseLabel(payload.phase) : undefined

  if (kind === 'phase_start') {
    return {
      kind: 'thinking',
      label: phase ?? 'Thinking',
      detail: capability,
    }
  }

  if (kind === 'phase_end') {
    return {
      kind: 'thinking',
      label: `${phase ?? 'Phase'} complete`,
      detail: capability,
    }
  }

  if (kind === 'tool_call') {
    return {
      kind: 'tool',
      label: `Using ${String(payload.tool ?? 'tool')}`,
      detail: typeof payload.step_id === 'string' ? `Step ${payload.step_id}` : capability,
    }
  }

  if (kind === 'tool_result') {
    return {
      kind: 'thinking',
      label: `${String(payload.tool ?? 'Tool')} finished`,
      detail: payload.ok === false ? 'Tool returned an error' : 'Reading result',
    }
  }

  if (kind === 'rag_citations') {
    const details = payload.details as { hits?: unknown } | undefined
    return {
      kind: 'tool',
      label: 'Sources attached',
      detail: typeof details?.hits === 'number' ? `${details.hits} citations` : capability,
    }
  }

  if (kind === 'replan') {
    return {
      kind: 'thinking',
      label: 'Replanning',
      detail: typeof payload.reason === 'string' ? payload.reason : undefined,
    }
  }

  if (kind === 'event_lagged') {
    return {
      kind: 'thinking',
      label: 'Catching up',
      detail: `Skipped ${String(payload.skipped ?? 0)} stale events`,
    }
  }

  return { kind: 'thinking', label: 'Working', detail: capability }
}

function citationsFromTrace(payload: Record<string, unknown>): Citation[] {
  const knowledgeCitations = knowledgeCitationsFromTrace(payload)
  if (knowledgeCitations.length > 0) return knowledgeCitations

  const isRagToolResult = payload.kind === 'tool_result' && payload.tool === 'rag_search'
  const isWebToolResult =
    payload.kind === 'tool_result' && (payload.tool === 'web_search' || payload.tool === 'web_fetch')
  const isRagCitationEvent = payload.kind === 'rag_citations'
  if (!isRagToolResult && !isWebToolResult && !isRagCitationEvent) return []
  const details = payload.details
  if (!details || typeof details !== 'object') return []
  const sources = (details as { sources?: unknown }).sources
  if (!Array.isArray(sources)) return []
  return sources
    .map((source): Citation | null => {
      if (!source || typeof source !== 'object') return null
      const item = source as Record<string, unknown>
      const url = typeof item.url === 'string' ? item.url : undefined
      const title = typeof item.title === 'string' ? item.title : undefined
      const sourceName =
        typeof item.source === 'string' && item.source.trim()
          ? item.source
          : title || url || 'source'
      const text =
        typeof item.text === 'string' && item.text.trim()
          ? item.text
          : typeof item.summary === 'string' && item.summary.trim()
            ? item.summary
            : typeof item.snippet === 'string' && item.snippet.trim()
              ? item.snippet
              : url || sourceName
      return {
        index: typeof item.index === 'number' ? item.index : 0,
        source: sourceName,
        text,
        kind: item.kind === 'web' || url ? 'web' : 'rag',
        title,
        url,
        score: typeof item.score === 'number' ? item.score : null,
        kb: typeof item.kb === 'string' ? item.kb : undefined,
        documentId: typeof item.document_id === 'string' ? item.document_id : undefined,
        chunkId: typeof item.chunk_id === 'string' ? item.chunk_id : typeof item.id === 'string' ? item.id : undefined,
        rawSource: typeof item.raw_source === 'string' ? item.raw_source : undefined,
        page: typeof item.page === 'string' || typeof item.page === 'number' ? item.page : undefined,
      }
    })
    .filter((source): source is Citation => Boolean(source && (source.text || source.url)))
}

function mergeCitations(existing: Citation[], incoming: Citation[]): Citation[] {
  const merged = [...existing]
  for (const citation of incoming) {
    const key = citation.url || `${citation.source}:${citation.text.slice(0, 80)}`
    const seen = merged.some((item) => (item.url || `${item.source}:${item.text.slice(0, 80)}`) === key)
    if (!seen) {
      merged.push({ ...citation, index: merged.length + 1 })
    }
  }
  return merged
}

function notebookEditProposalFromTrace(payload: Record<string, unknown>): NotebookEditProposal | undefined {
  if (payload.kind !== 'tool_result' || payload.tool !== 'propose_notebook_edit' || payload.ok === false) return undefined
  const details = payload.details
  if (!details || typeof details !== 'object') return undefined
  const item = details as Record<string, unknown>
  if (item.found === false) return undefined
  const entryId = typeof item.entry_id === 'string' ? item.entry_id : ''
  const proposedMarkdown = typeof item.proposed_markdown === 'string' ? item.proposed_markdown : ''
  if (!entryId || !proposedMarkdown.trim()) return undefined
  const entryTitle = typeof item.entry_title === 'string' ? item.entry_title : 'Notebook entry'
  return {
    entryId,
    entryTitle,
    proposedTitle: typeof item.proposed_title === 'string' && item.proposed_title.trim() ? item.proposed_title : entryTitle,
    proposedMarkdown,
    summary: typeof item.summary === 'string' && item.summary.trim() ? item.summary : 'Proposed Notebook update',
    proposalKind: notebookProposalKind(item.proposal_kind),
    suggestedLinks: notebookSuggestedLinks(item.suggested_links),
    suggestedTags: notebookSuggestedTags(item.suggested_tags),
    mergeSourceEntryIds: notebookMergeSourceEntryIds(item.merge_source_entry_ids),
  }
}

function notebookProposalKind(value: unknown): NotebookEditProposal['proposalKind'] {
  return value === 'links' || value === 'tags' || value === 'merge' || value === 'edit' ? value : 'edit'
}

function notebookSuggestedLinks(value: unknown): NotebookEditProposal['suggestedLinks'] {
  if (!Array.isArray(value)) return []
  const links: NonNullable<NotebookEditProposal['suggestedLinks']> = []
  for (const item of value) {
    if (!item || typeof item !== 'object') continue
    const record = item as Record<string, unknown>
    const text = typeof record.text === 'string' ? record.text.trim() : ''
    const target = typeof record.target === 'string' ? record.target.trim() : ''
    if (!text || !target) continue
    links.push({
      text,
      target,
      reason: typeof record.reason === 'string' && record.reason.trim() ? record.reason.trim() : undefined,
    })
  }
  return links
}

function notebookSuggestedTags(value: unknown): NotebookEditProposal['suggestedTags'] {
  if (!Array.isArray(value)) return []
  const tags: NonNullable<NotebookEditProposal['suggestedTags']> = []
  for (const item of value) {
    if (!item || typeof item !== 'object') continue
    const record = item as Record<string, unknown>
    const tag = typeof record.tag === 'string' ? record.tag.trim().replace(/^#/, '') : ''
    const action = record.action
    if (!tag || (action !== 'add' && action !== 'keep' && action !== 'remove')) continue
    tags.push({
      tag,
      action,
      reason: typeof record.reason === 'string' && record.reason.trim() ? record.reason.trim() : undefined,
    })
  }
  return tags
}

function notebookMergeSourceEntryIds(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter(Boolean)
}

function restoreTraceEntries(
  trace: NonNullable<SessionDetailResponse['trace']>,
  compactSummary: NonNullable<SessionDetailResponse['compact_summary']> | null,
): TraceEntry[] {
  const entries: TraceEntry[] = trace.map((entry) => {
    const payload = {
      ...(entry.payload ?? {}),
      kind: entry.kind,
    }
    return {
      kind: entry.kind,
      payload,
      timestamp: entry.timestamp ? Date.parse(entry.timestamp) : Date.now(),
    }
  })

  if (compactSummary?.summary) {
    const payload: Record<string, unknown> = {
      kind: 'compact_summary',
      summary: compactSummary.summary,
      message_count: compactSummary.message_count,
    }
    entries.unshift({
      kind: 'compact_summary',
      payload,
      timestamp: compactSummary.timestamp ? Date.parse(compactSummary.timestamp) : Date.now(),
    })
  }

  return entries
}

function attachRestoredCitations(messages: Message[], traceEntries: TraceEntry[]): Message[] {
  const citationGroups = traceEntries
    .filter((entry) => {
      const payload = entry.payload as Record<string, unknown>
      return (
        entry.kind === 'rag_citations' ||
        (entry.kind === 'tool_result' &&
          (payload.tool === 'rag_search' ||
            payload.tool === 'knowledge_read' ||
            payload.tool === 'web_search' ||
            payload.tool === 'web_fetch'))
      )
    })
    .map((entry) => citationsFromTrace(entry.payload))
    .filter((citations) => citations.length > 0)

  if (citationGroups.length === 0) return messages

  let citationIndex = 0
  return messages.map((message) => {
    if (message.role !== 'assistant') return message
    if (message.citations && message.citations.length > 0) return message
    const citations = citationGroups[citationIndex]
    citationIndex += 1
    return citations ? { ...message, citations } : message
  })
}

function deepSolveEventFromTrace(payload: Record<string, unknown>, timestamp = Date.now()): DeepSolveTraceEntry | null {
  const kind = typeof payload.kind === 'string' ? payload.kind : ''
  const capability = typeof payload.capability === 'string' ? payload.capability : ''
  if (!kind.startsWith('deep_solve_') && capability !== 'deep_solve') return null

  return {
    kind,
    payload,
    timestamp,
  }
}

function attachRestoredDeepSolve(messages: Message[], traceEntries: TraceEntry[]): Message[] {
  const deepSolveEvents = traceEntries
    .map((entry) => deepSolveEventFromTrace(entry.payload, entry.timestamp))
    .filter((entry): entry is DeepSolveTraceEntry => Boolean(entry))

  if (deepSolveEvents.length === 0) return messages

  const groups: DeepSolveTraceEntry[][] = []
  let current: DeepSolveTraceEntry[] = []
  for (const event of deepSolveEvents) {
    current.push(event)
    if (event.kind === 'deep_solve_final') {
      groups.push(current)
      current = []
    }
  }
  if (current.length > 0) {
    groups.push(current)
  }

  let groupIndex = 0
  return messages.map((message) => {
    if (message.role !== 'assistant') return message
    const group = groups[groupIndex]
    if (!group) return message
    groupIndex += 1
    return { ...message, deepSolve: group }
  })
}

function capabilityLabel(value: string): string {
  if (value === 'deep_solve') return 'Deep Solve'
  if (value === 'code_exec') return 'Code Exec'
  if (value === 'organize') return 'Organize'
  return 'Chat'
}

function phaseLabel(value: string): string {
  const labels: Record<string, string> = {
    respond: 'Responding',
    execute: 'Executing code',
    pre_retrieve: 'Preparing knowledge',
    plan: 'Planning',
    solve_steps: 'Solving steps',
    solve_step: 'Solving step',
    synthesize: 'Synthesizing answer',
  }
  return labels[value] ?? value
}

function sessionTitleFromMessage(text: string) {
  const normalized = text.replace(/\s+/g, ' ').trim()
  if (!normalized) return '新的会话'
  return normalized.length > 18 ? `${normalized.slice(0, 18)}...` : normalized
}

function promoteRecentSession(
  setRecentSessions: Dispatch<SetStateAction<RecentSession[]>>,
  id: string,
  title: string,
  temporary = false,
) {
  setRecentSessions((prev) => {
    const existing = prev.find((session) => session.id === id)
    const nextSession = {
      id,
      title,
      activeRun: existing?.activeRun ?? null,
      pinned: existing?.pinned ?? false,
      temporary: existing?.temporary ?? temporary,
    }
    const rest = prev.filter((session) => session.id !== id)
    return nextSession.pinned
      ? sortRecentSessions([nextSession, ...rest])
      : [
        ...rest.filter((session) => session.pinned),
        nextSession,
        ...rest.filter((session) => !session.pinned),
      ]
  })
}

function updateRecentSessionTitle(
  setRecentSessions: Dispatch<SetStateAction<RecentSession[]>>,
  id: string,
  title: string,
) {
  setRecentSessions((prev) =>
    prev.map((session) => (session.id === id ? { ...session, title } : session)),
  )
}

function sortRecentSessions(sessions: RecentSession[]) {
  return [
    ...sessions.filter((session) => session.pinned),
    ...sessions.filter((session) => !session.pinned),
  ]
}

const pinnedSessionsStorageKey = 'llm-tutor:pinned-sessions'

function loadPinnedSessionIds() {
  try {
    const raw = window.localStorage.getItem(pinnedSessionsStorageKey)
    const parsed = raw ? JSON.parse(raw) : []
    return new Set(Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [])
  } catch {
    return new Set<string>()
  }
}

function savePinnedSessionIds(ids: Set<string>) {
  window.localStorage.setItem(pinnedSessionsStorageKey, JSON.stringify([...ids]))
}

function updateRecentSessionRun(
  setRecentSessions: Dispatch<SetStateAction<RecentSession[]>>,
  id: string,
  activeRun: SessionRunSummary | null,
) {
  setRecentSessions((prev) =>
    prev.map((session) => (session.id === id ? { ...session, activeRun } : session)),
  )
}

function runSummaryFromStatusPayload(payload: Record<string, unknown>): SessionRunSummary {
  const capability = typeof payload.capability === 'string' && isCapability(payload.capability)
    ? payload.capability
    : undefined
  return {
    run_id: typeof payload.run_id === 'string' ? payload.run_id : undefined,
    capability,
    status: typeof payload.status === 'string' ? payload.status : 'running',
    current_stage: typeof payload.current_stage === 'string' ? payload.current_stage : null,
    started_at: typeof payload.started_at === 'string' ? payload.started_at : undefined,
    updated_at: typeof payload.updated_at === 'string' ? payload.updated_at : undefined,
  }
}

function estimateContextTokens(messages: Message[], streamingText: string) {
  const text = [
    ...messages
      .filter((message) => message.role === 'user' || message.role === 'assistant')
      .map((message) => message.text),
    streamingText,
  ].join('\n')
  if (!text.trim()) return 0

  let ascii = 0
  let nonAscii = 0
  for (const char of text) {
    if (char.charCodeAt(0) <= 0x7f) ascii += 1
    else nonAscii += 1
  }

  const messageOverhead = messages.filter((message) => message.role === 'user' || message.role === 'assistant').length * 4
  return Math.ceil(ascii / 4 + nonAscii * 1.2 + messageOverhead)
}

function buildMessageContentWithAttachments(text: string, attachments: ChatAttachment[]) {
  const baseText = text.trim()
  const source = attachmentSourceText(attachments)
  if (!source) return baseText
  return `${baseText || 'Please continue based on the attachment content.'}\n\n${source}`
}

function attachmentSourceText(attachments: ChatAttachment[]) {
  const readable = attachments.filter((attachment) => attachment.text?.trim())
  if (readable.length === 0) return ''
  return [
    '[Attachment context]',
    ...readable.map((attachment, index) => [
      `### ${index + 1}. ${attachment.name}`,
      `Type: ${attachment.type || 'unknown'}`,
      `Size: ${formatBytes(attachment.size)}`,
      attachment.truncated ? 'Note: content was truncated.' : null,
      '',
      attachment.text?.trim() ?? '',
    ].filter(Boolean).join('\n')),
  ].join('\n\n')
}


function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function isTokenUsagePayload(value: unknown): value is TokenUsagePayload {
  return Boolean(value && typeof value === 'object')
}

function tokenUsageFromRuntimeTrace(payload: Record<string, unknown>): TokenUsagePayload | null {
  if (payload.kind !== 'runtime_usage') return null
  const inputTokens = numberOrUndefined(payload.input_tokens)
  const outputTokens = numberOrUndefined(payload.output_tokens)
  const cacheReadTokens = numberOrUndefined(payload.cache_read_tokens)
  const cacheCreationTokens = numberOrUndefined(payload.cache_write_tokens)
  const tokenParts = [
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    numberOrUndefined(payload.reasoning_tokens),
  ].filter((value): value is number => typeof value === 'number')
  const totalTokens = tokenParts.reduce((sum, value) => sum + value, 0)

  return {
    input_tokens: inputTokens,
    output_tokens: outputTokens,
    cache_read_tokens: cacheReadTokens,
    cache_creation_tokens: cacheCreationTokens,
    total_tokens: totalTokens,
    source: 'runtime',
  }
}

function numberOrUndefined(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function isCapability(value: string): value is Capability {
  return value === 'chat' || value === 'deep_solve' || value === 'code_exec' || value === 'organize'
}

function sourceTargetDetail(target: SourceTarget, reference: SourceReference) {
  if (target.type === 'notebook') return `Notebook ${target.entryId}`
  if (target.type === 'kb') return target.chunkId ? `Knowledge ${target.documentId}, chunk ${target.chunkId}` : `Knowledge ${target.documentId}`
  return reference.raw
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

async function persistMessageCitations(sessionId: string, citations: Citation[]) {
  const res = await fetch(`/api/sessions/${sessionId}/message-citations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ citations }),
  })
  const data = await safeJson(res)
  if (!res.ok) {
    throw new Error(errorMessage(data, res.status))
  }
}
