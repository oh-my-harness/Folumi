import { useState, useCallback, useEffect, useMemo, useRef } from 'react'
import type { Dispatch, SetStateAction } from 'react'
import { ChatBox } from './components/ChatBox'
import type { ChatAttachment, ContextStats, NotebookEditProposal, SaveToNotebookOptions, SpaceMention } from './components/ChatBox'
import { TracePanel, TraceEntry } from './components/TracePanel'
import { BudgetPanel } from './components/BudgetPanel'
import { ApprovalDialog } from './components/ApprovalDialog'
import { SettingsPage, type SettingsTab } from './components/SettingsPage'
import { KnowledgePage } from './components/KnowledgePage'
import { SpacePage } from './components/SpacePage'
import { MemoryPage } from './components/MemoryPage'
import { TutorPage } from './components/TutorPage'
import { OnboardingDialog } from './components/OnboardingDialog'
import { onboardingModeBlock, onboardingStarterPrompt, type OnboardingMode } from './onboardingModes'
import { OnboardingResumeButton } from './components/OnboardingResumeButton'
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
import type { QuizSession } from './quizTypes'
import { attachRestoredQuizzesToMessages, quizFromTrace } from './quizRestore'
import { attachRestoredResearchReports, researchReportFromTracePayload } from './researchRestore'
import type { ResearchReportTraceData } from './researchRestore'
import { guideTutorStarterPrompt, type ProductGuideDestination } from './productGuide'
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
import { BUILT_IN_GUIDE_TUTOR_ID, fetchTutors, type TutorProfile, type TutorSummary } from './tutorTypes'
import { tutorBindingForCreate } from './tutorSession'
import { knowledgeCitationsFromTrace } from './knowledgeCitation'

type Capability = 'chat' | 'deep_solve' | 'code_exec' | 'quiz' | 'research' | 'organize'

interface Message {
  role: 'user' | 'assistant' | 'status'
  text: string
  kind?: AgentStatus['kind']
  transient?: boolean
  citations?: Citation[]
  deepSolve?: DeepSolveTraceEntry[]
  quiz?: QuizSession
  artifacts?: MessageArtifact[]
  quizPlan?: QuizPlan
  researchPlan?: ResearchPlan
  researchTitle?: string
  researchUnavailable?: boolean
  notebookEditProposal?: NotebookEditProposal
  attachments?: ChatAttachment[]
  mentions?: SpaceMention[]
}

interface MessageArtifact {
  type: 'quiz_session' | string
  quiz_id?: string
  artifact_id?: string
  artifact_store?: string
  title?: string
}

interface QuizPlan {
  title: string
  topic: string
  source: string
  difficulty: string
  questionCount: number
  notes: string[]
}

interface ResearchPlan {
  title: string
  topic: string
  scope: string
  outputFormat: string
  depth: string
  timeRange: string
  sourcePreferences: string[]
  useNotebook: boolean
  useKnowledgeBase: boolean
  steps: string[]
  questions: string[]
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
  tutorId?: string | null
  tutor?: TutorSummary | null
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
    tutor_id?: string | null
    tutor?: TutorSummary | null
  }>
}

interface SessionDetailResponse {
  tutor_id?: string | null
  tutor?: TutorSummary | null
  capability?: Capability
  kb?: string | null
  notebook_enabled?: boolean
  llm?: { model?: string | null } | null
  messages?: Array<{
    role: 'user' | 'assistant'
    text: string
    mentions?: SpaceMention[]
    citations?: Citation[]
    artifacts?: MessageArtifact[]
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

export default function App() {
  const [view, setView] = useState<AppView>('chat')
  const [capability, setCapability] = useState<Capability>('chat')
  const [llmSettings, setLlmSettings] = useState(loadLlmSettings)
  const [settingsHydrated, setSettingsHydrated] = useState(false)
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const [onboardingStep, setOnboardingStep] = useState(0)
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('llm')
  const [starterDraft, setStarterDraft] = useState<{ id: number; text: string } | null>(null)
  const [selectedLlmConfigId, setSelectedLlmConfigId] = useState<string | null>(() => loadLlmSettings().activeLlmConfigId)
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [selectedTutorId, setSelectedTutorId] = useState<string | null>(null)
  const [tutors, setTutors] = useState<TutorProfile[]>([])
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
  const pendingSessionSendRef = useRef<{ sessionId: string; content: string; mentions: SpaceMention[] } | null>(null)
  const [traceEntries, setTraceEntries] = useState<TraceEntry[]>([])
  const pendingCitationsRef = useRef<Citation[]>([])
  const pendingDeepSolveRef = useRef<DeepSolveTraceEntry[]>([])
  const pendingNotebookEditProposalRef = useRef<NotebookEditProposal | undefined>(undefined)
  const pendingQuizRef = useRef<QuizSession | undefined>(undefined)
  const pendingQuizPlanRef = useRef<QuizPlan | undefined>(undefined)
  const pendingResearchPlanRef = useRef<ResearchPlan | undefined>(undefined)
  const pendingResearchReportRef = useRef<ResearchReportTraceData | undefined>(undefined)
  const [budgetSpent, setBudgetSpent] = useState(0)
  const [budgetWarning, setBudgetWarning] = useState(false)
  const [pendingApproval, setPendingApproval] = useState<{ tool: string; args: Record<string, unknown>; requestId: string } | null>(null)
  const [running, setRunning] = useState(false)
  const [recentSessions, setRecentSessions] = useState<RecentSession[]>([])
  const [pinnedSessionIds, setPinnedSessionIds] = useState<Set<string>>(() => loadPinnedSessionIds())
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBaseOption[]>([])
  const [notebookFolders, setNotebookFolders] = useState<string[]>([])
  const [notebookEntryPaths, setNotebookEntryPaths] = useState<string[]>([])
  const [notebookVault, setNotebookVault] = useState<NotebookVaultInfo | null>(null)
  const [selectedKnowledgeBaseId, setSelectedKnowledgeBaseId] = useState<string>('')
  const [selectedNotebookEnabled, setSelectedNotebookEnabled] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [traceCollapsed, setTraceCollapsed] = useState(true)
  const [spaceFocusTarget, setSpaceFocusTarget] = useState<Extract<SourceTarget, { type: 'notebook' | 'quiz' }> | null>(null)
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
          mentions: message.mentions,
          citations: message.citations,
          artifacts: message.artifacts,
        })),
        restoredTrace,
      )
      const restoredReports = attachRestoredResearchReports(withCitations, restoredTrace)
      const restored = attachRestoredResearchPlans(
        attachRestoredQuizPlans(attachRestoredDeepSolve(restoredReports, restoredTrace), restoredTrace),
        restoredTrace,
      )
      setMessages((live) => reconcileSessionMessages(restored, live))
      void attachRestoredQuizzes(restored, restoredTrace).then((nextMessages) => {
        if (isCurrentHydration()) {
          setMessages((live) => reconcileSessionMessages(nextMessages, live))
        }
      })
      setTraceEntries(restoredTrace)
      setLatestUsage(data.latest_usage ?? null)
      setSelectedTutorId(data.tutor_id ?? null)
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
      setSelectedKnowledgeBaseId(data.kb ?? '')
      setSelectedNotebookEnabled(Boolean(data.notebook_enabled))
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
  }, [llmSettings, pushStatus])

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
          const quiz = pendingQuizRef.current
          const quizPlan = pendingQuizPlanRef.current
          const researchPlan = pendingResearchPlanRef.current
          const researchReport = pendingResearchReportRef.current
          const messageText = researchReport?.markdown || finalText || (quiz ? `Quiz "${quiz.title}" is ready.` : '')
          if (messageText.trim() || citations.length > 0 || deepSolve.length > 0 || notebookEditProposal || quiz || quizPlan || researchPlan) {
            setMessages((prev) => appendCompletedSessionMessage(
              dropTrailingTransientStatus(prev),
              {
                role: 'assistant',
                text: messageText,
                citations,
                deepSolve: deepSolve.length > 0 ? deepSolve : undefined,
                notebookEditProposal,
                quiz,
                artifacts: quiz ? [{ type: 'quiz_session', quiz_id: quiz.id }] : undefined,
                quizPlan,
                researchPlan,
                researchTitle: researchReport?.title,
              },
            ))
          } else {
            setMessages((prev) => dropTrailingTransientStatus(prev))
          }
          pendingCitationsRef.current = []
          pendingDeepSolveRef.current = []
          pendingNotebookEditProposalRef.current = undefined
          pendingQuizRef.current = undefined
          pendingQuizPlanRef.current = undefined
          pendingResearchPlanRef.current = undefined
          pendingResearchReportRef.current = undefined
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
          if (typeof event.payload.cost_usd === 'number') {
            setBudgetSpent(event.payload.cost_usd)
          }
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
        const quiz = quizFromTrace(event.payload as Record<string, unknown>)
        if (quiz) {
          pendingQuizRef.current = quiz
        }
        const quizPlan = quizPlanFromTrace(event.payload as Record<string, unknown>)
        if (quizPlan) {
          pendingQuizPlanRef.current = quizPlan
        }
        const researchPlan = researchPlanFromTrace(event.payload as Record<string, unknown>)
        if (researchPlan) {
          pendingResearchPlanRef.current = researchPlan
        }
        const researchReport = researchReportFromTrace(event.payload as Record<string, unknown>)
        if (researchReport) {
          pendingResearchReportRef.current = researchReport
        }
        pushStatus(statusFromTrace(event.payload as Record<string, unknown>))
        setTraceEntries((prev) => [
          ...prev,
          { kind: event.payload.kind, payload: event.payload, timestamp: Date.now() },
        ])
      } else if (event.type === 'status') {
        const payload = event.payload as Record<string, unknown>
        const kind = payload.kind as string
        if (kind === 'budget_warning') {
          setBudgetWarning(true)
          setBudgetSpent((payload.spent_usd as number) ?? 0)
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
          setPendingApproval(null)
          setRunning(false)
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
          setLatestUsage((prev) => isTokenUsagePayload(payload.usage) ? payload.usage : prev)
          if (!streamingRef.current) {
            pushStatus({
              kind: 'done',
              label: 'Done',
              detail: typeof payload.history_len === 'number' ? `${payload.history_len} context messages` : undefined,
            })
          }
        } else if (kind === 'history_sync') {
          setPendingApproval(null)
          streamingRef.current = ''
          progressStreamingRef.current = ''
          setStreamingText('')
          setRunning(false)
          setMessages((prev) => dropTrailingTransientStatus(prev))
          updateRecentSessionRun(setRecentSessions, sourceSessionId, null)
          void hydrateSession(sourceSessionId, true)
        } else if (kind === 'stopped') {
          setPendingApproval(null)
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
        } else if (kind === 'approval_request') {
          pushStatus({
            kind: 'tool',
            label: 'Waiting for approval',
            detail: typeof payload.tool === 'string' ? payload.tool : undefined,
          })
          setPendingApproval({
            tool: payload.tool as string,
            args: payload.args as Record<string, unknown>,
            requestId: payload.request_id as string,
          })
        } else if (kind === 'error') {
          setPendingApproval(null)
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
      setPendingApproval(null)
      setRunning(false)
      pushStatus({ kind: 'idle', label: 'Disconnected', detail: 'WebSocket closed' })
    },
    onError: (sourceSessionId) => {
      if (!isCurrentSessionEvent(sourceSessionId, activeSessionIdRef.current)) return
      setPendingApproval(null)
      pushStatus({ kind: 'error', label: 'Connection failed', detail: 'Check tutor-web server' })
      setMessages((prev) => [
        ...prev,
        { role: 'assistant', text: 'Error: WebSocket connection failed. Check that tutor-web is running on 127.0.0.1:8080.' },
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
      tutorId: session.tutor_id ?? null,
      tutor: session.tutor ?? null,
    }))))
  }, [pinnedSessionIds])

  const refreshTutors = useCallback(async () => {
    setTutors(await fetchTutors())
  }, [])

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
    setSelectedKnowledgeBaseId((current) => {
      if (current && items.some((item) => item.id === current)) return current
      return ''
    })
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
  }, [])

  useEffect(() => {
    const pending = pendingSessionSendRef.current
    if (!pending || pending.sessionId !== sessionId) return
    pendingSessionSendRef.current = null
    send({ type: 'message', content: pending.content, mentions: pending.mentions })
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
    refreshTutors().catch((err) => {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: '导师加载失败', detail: message })
    })
  }, [refreshSessions, refreshKnowledgeBases, refreshNotebookFolders, refreshTutors, pushStatus])

  const handleSend = useCallback(async (text: string, attachments: ChatAttachment[] = [], mentions: SpaceMention[] = []) => {
    try {
      const tutorBinding = sessionId ? null : tutorBindingForCreate(selectedTutorId)
      const content = buildMessageContentWithAttachments(text, attachments)
      const displayText = text.trim() || (attachments.length > 0 ? `Sent ${attachments.length} attachment(s)` : `Referenced ${mentions.length} Space item(s)`)
      let sid = sessionId
      let createdSession = false
      if (!sid) {
        const kb = selectedKnowledgeBaseId || null
        const res = await fetch('/api/sessions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            capability,
            ...tutorBinding,
            kb,
            notebook_enabled: selectedNotebookEnabled,
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
        pendingSessionSendRef.current = { sessionId: createdSessionId, content, mentions }
        activateSession(createdSessionId)
        promoteRecentSession(
          setRecentSessions,
          createdSessionId,
          sessionTitleFromMessage(displayText),
          selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) ?? null : null,
        )
      } else {
        promoteRecentSession(setRecentSessions, sid, sessionTitleFromMessage(displayText))
      }

      setMessages((prev) => [...prev, { role: 'user', text: displayText, attachments, mentions }])
      setRunning(true)
      pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
      if (!createdSession) send({ type: 'message', content, mentions }, sid)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
      setRunning(false)
    }
  }, [sessionId, selectedTutorId, tutors, capability, llmSettings, selectedLlmConfigId, selectedKnowledgeBaseId, selectedNotebookEnabled, send, pushStatus, activateSession])

  const handleStopGeneration = useCallback(() => {
    if (!running) return
    setPendingApproval(null)
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
          pendingQuizRef.current = undefined
          pendingQuizPlanRef.current = undefined
          pendingResearchPlanRef.current = undefined
          pendingResearchReportRef.current = undefined
          setRunning(true)
          pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
          send({ type: 'message', content: nextText, mentions: [] })
          promoteRecentSession(setRecentSessions, sessionId, sessionTitleFromMessage(nextText))
          return
        }
      }

      const res = await fetch('/api/sessions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          capability,
          ...tutorBindingForCreate(selectedTutorId),
          kb: selectedKnowledgeBaseId || null,
          notebook_enabled: selectedNotebookEnabled,
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
      promoteRecentSession(
        setRecentSessions,
        nextSessionId,
        sessionTitleFromMessage(nextText),
        selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) ?? null : null,
      )
      setMessages([{ role: 'user', text: nextText }])
      setTraceEntries([])
      setLatestUsage(null)
      setStreamingText('')
      streamingRef.current = ''
      progressStreamingRef.current = ''
      pendingCitationsRef.current = []
      pendingDeepSolveRef.current = []
      pendingNotebookEditProposalRef.current = undefined
      pendingQuizRef.current = undefined
      pendingQuizPlanRef.current = undefined
      pendingResearchPlanRef.current = undefined
      pendingResearchReportRef.current = undefined
      setRunning(true)
      pushStatus({ kind: 'thinking', label: 'Thinking', detail: capabilityLabel(capability) })
      pendingSessionSendRef.current = { sessionId: nextSessionId, content: nextText, mentions: [] }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Error', detail: message })
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
      setRunning(false)
    }
  }, [capability, llmSettings, selectedLlmConfigId, messages, pushStatus, running, selectedKnowledgeBaseId, selectedNotebookEnabled, selectedTutorId, tutors, send, sessionId, activateSession])

  const updateQuizInMessages = useCallback((quiz: QuizSession) => {
    setMessages((prev) =>
      prev.map((message) => {
        if (message.quiz?.id !== quiz.id) return message
        return { ...message, quiz }
      }),
    )
  }, [])

  const handleQuizAnswer = useCallback(async (quizId: string, questionId: string, selectedOptionId: string) => {
    const res = await fetch(`/api/quizzes/${encodeURIComponent(quizId)}/answers`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        question_id: questionId,
        selected_option_id: selectedOptionId,
      }),
    })
    const data = await safeJson(res)
    if (!res.ok) {
      throw new Error(errorMessage(data, res.status))
    }
    updateQuizInMessages(data.quiz as QuizSession)
  }, [updateQuizInMessages])

  const handleQuizFinish = useCallback(async (quizId: string) => {
    const res = await fetch(`/api/quizzes/${encodeURIComponent(quizId)}/finish`, { method: 'POST' })
    const data = await safeJson(res)
    if (!res.ok) {
      throw new Error(errorMessage(data, res.status))
    }
    updateQuizInMessages(data.quiz as QuizSession)
  }, [updateQuizInMessages])

  const handleSaveToNotebook = useCallback(async (markdown: string, options: SaveToNotebookOptions = {}): Promise<SaveToNotebookResult> => {
    try {
      const title = options.title?.trim() || titleFromMarkdown(markdown)
      const entryType = resolveGeneratedNotebookEntryType(capability, options.entryType)
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
            generatedBy: entryType === 'research_report' ? 'research' : 'chat',
            ...(entryType === 'research_report' ? { reportVersion: 1 } : {}),
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
  }, [capability, pushStatus, refreshNotebookFolders, sessionId])

  const handleApplyNotebookEdit = useCallback(async (proposal: NotebookEditProposal) => {
    try {
      const res = await fetch(`/api/notebook/entries/${encodeURIComponent(proposal.entryId)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
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

  const handleRegenerateResearch = useCallback((markdown: string) => {
    const title = titleFromMarkdown(markdown)
    const prompt = [
      'Start the detailed research workflow to regenerate this report as a new version.',
      `Previous report title: ${title}`,
      'Refresh the search, read current or better sources, re-check citations, and return a new final Markdown report.',
      'Keep the same general scope unless newer evidence suggests a better framing.',
      '',
      'Previous report:',
      markdown,
    ].join('\n')
    void handleSend(prompt)
  }, [handleSend])

  const handleIngestResearchSources = useCallback(async (sources: SourceReference[], markdown: string) => {
    if (!selectedKnowledgeBaseId) {
      pushStatus({ kind: 'error', label: 'No Knowledge Base', detail: 'Select a Knowledge Base before adding report sources.' })
      return
    }
    const usableSources = sources.filter((source) => source.target?.type === 'web' || source.metadata?.url || source.raw)
    if (usableSources.length === 0) {
      pushStatus({ kind: 'error', label: 'No sources', detail: 'This report has no importable sources.' })
      return
    }
    pushStatus({ kind: 'tool', label: 'Adding sources', detail: `${usableSources.length} source(s)` })
    try {
      for (const source of usableSources) {
        const sourceUrl = source.target?.type === 'web' ? source.target.url : source.metadata?.url
        const text = researchSourceIngestText(source, markdown)
        const res = await fetch(`/api/knowledge-bases/${encodeURIComponent(selectedKnowledgeBaseId)}/documents`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            source: sourceUrl || source.title || source.raw || 'research-source',
            text,
          }),
        })
        const data = await safeJson(res)
        if (!res.ok) throw new Error(errorMessage(data, res.status))
        const job = data.job && typeof data.job === 'object' ? data.job as Record<string, unknown> : null
        await pollIngestionJob(job?.id)
      }
      await refreshKnowledgeBases()
      pushStatus({ kind: 'done', label: 'Sources added', detail: `${usableSources.length} source(s) indexed` })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      pushStatus({ kind: 'error', label: 'Source import failed', detail: message })
    }
  }, [pushStatus, refreshKnowledgeBases, selectedKnowledgeBaseId])

  const handleSettingsChange = (nextSettings: typeof llmSettings) => {
    setLlmSettings(nextSettings)
    const tutor = selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) : null
    const tutorModelExists = tutor?.default_model_config_id
      ? nextSettings.llmConfigs.some((config) => config.id === tutor.default_model_config_id)
      : false
    setSelectedLlmConfigId(
      tutorModelExists ? tutor?.default_model_config_id ?? nextSettings.activeLlmConfigId : nextSettings.activeLlmConfigId,
    )
    persistSettings(nextSettings)
    if (settingsRequireSessionReset(llmSettings, nextSettings)) {
      activateSession(null)
      setSelectedTutorId(null)
    }
  }

  const startNewChat = useCallback(() => {
    activateSession(null)
    setSelectedTutorId(null)
    setSelectedLlmConfigId(llmSettings.activeLlmConfigId)
    setMessages([])
    setStreamingText('')
    streamingRef.current = ''
    progressStreamingRef.current = ''
    setTraceEntries([])
    pendingCitationsRef.current = []
    pendingDeepSolveRef.current = []
    pendingNotebookEditProposalRef.current = undefined
    pendingQuizRef.current = undefined
    pendingQuizPlanRef.current = undefined
    pendingResearchPlanRef.current = undefined
    pendingResearchReportRef.current = undefined
    setLatestUsage(null)
    setBudgetWarning(false)
    setRunning(false)
    setView('chat')
  }, [activateSession, llmSettings.activeLlmConfigId])

  const handleTutorSelect = useCallback((tutorId: string | null) => {
    setSelectedTutorId(tutorId)
    const tutor = tutorId ? tutors.find((item) => item.id === tutorId) : null
    setSelectedLlmConfigId(tutor?.default_model_config_id ?? llmSettings.activeLlmConfigId)
    if (tutor) {
      setSelectedKnowledgeBaseId((current) => (
        tutor.resource_permissions.knowledge_base_ids.includes(current) ? current : ''
      ))
      if (!tutor.resource_permissions.notebook) setSelectedNotebookEnabled(false)
    }
    const nextCapability = tutorId
      ? tutor?.default_capability
      : 'chat'
    if (nextCapability && isCapability(nextCapability)) {
      setCapability(nextCapability)
    }
  }, [llmSettings.activeLlmConfigId, tutors])

  const handleNavigate = useCallback((nextView: AppView) => {
    if (nextView === 'chat') {
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

  const startOnboardingMode = useCallback((mode: OnboardingMode) => {
    const tutorId = selectedTutorId
    const tutor = tutorId ? tutors.find((item) => item.id === tutorId) ?? null : null
    const block = onboardingModeBlock(mode, tutor)
    if (block === 'capability') {
      pushStatus({ kind: 'error', label: '模式不可用', detail: '当前导师未启用此模式。' })
      return
    }
    if (block === 'notebook') {
      pushStatus({ kind: 'error', label: 'Notebook 不可用', detail: '当前导师没有 Notebook 权限。' })
      return
    }

    completeOnboarding()
    startNewChat()
    handleTutorSelect(tutorId)
    setCapability(mode)
    if (mode === 'organize') {
      setSelectedNotebookEnabled(true)
      setSelectedKnowledgeBaseId('')
    }
    setStarterDraft({ id: Date.now(), text: onboardingStarterPrompt(mode, llmSettings.language) })
  }, [completeOnboarding, handleTutorSelect, llmSettings.language, pushStatus, selectedTutorId, startNewChat, tutors])

  const startGuideTutor = useCallback(() => {
    const guideTutor = tutors.find((tutor) => tutor.id === BUILT_IN_GUIDE_TUTOR_ID)
    if (!guideTutor) {
      pushStatus({ kind: 'error', label: '使用指南不可用', detail: '内置使用指南 Tutor 尚未加载，请稍后重试。' })
      return
    }
    startNewChat()
    handleTutorSelect(guideTutor.id)
    setCapability('chat')
    setSelectedKnowledgeBaseId('')
    setSelectedNotebookEnabled(false)
    setStarterDraft({ id: Date.now(), text: guideTutorStarterPrompt(llmSettings.language) })
  }, [handleTutorSelect, llmSettings.language, pushStatus, startNewChat, tutors])

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
    setView(destination === 'tutors' ? 'tutor' : destination)
  }, [startNewChat])

  const handleCapabilityChange = useCallback(async (nextCapability: Capability) => {
    if (running) return
    const tutor = selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) : null
    if (tutor && !tutor.allowed_capabilities.includes(nextCapability)) {
      pushStatus({ kind: 'error', label: '模式不可用', detail: '当前导师未启用此能力。' })
      return
    }
    if (nextCapability === 'organize' && tutor && !tutor.resource_permissions.notebook) {
      pushStatus({ kind: 'error', label: 'Notebook 不可用', detail: '当前导师没有 Notebook 权限。' })
      return
    }

    setCapability(nextCapability)
    if (nextCapability === 'organize') {
      setSelectedNotebookEnabled(true)
      setSelectedKnowledgeBaseId('')
    }
    if (!sessionId) return

    try {
      const body = nextCapability === 'organize'
        ? { capability: nextCapability, notebook_enabled: true, kb: '' }
        : { capability: nextCapability }
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
      if (!res.ok) {
        throw new Error(`failed to update session mode: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [pushStatus, running, selectedTutorId, sessionId, tutors])

  const handleKnowledgeBaseChange = useCallback(async (nextKb: string) => {
    if (running) return
    const tutor = selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) : null
    if (nextKb && tutor && !tutor.resource_permissions.knowledge_base_ids.includes(nextKb)) {
      pushStatus({ kind: 'error', label: '知识库不可用', detail: '当前导师没有该知识库的权限。' })
      return
    }
    setSelectedKnowledgeBaseId(nextKb)
    setSelectedNotebookEnabled(false)
    if (!sessionId) return

    try {
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ kb: nextKb, notebook_enabled: false }),
      })
      if (!res.ok) {
        throw new Error(`failed to update session knowledge base: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [pushStatus, running, selectedTutorId, sessionId, tutors])

  const handleNotebookEnabledChange = useCallback(async (enabled: boolean) => {
    if (running) return
    const tutor = selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) : null
    if (enabled && tutor && !tutor.resource_permissions.notebook) {
      pushStatus({ kind: 'error', label: 'Notebook 不可用', detail: '当前导师没有 Notebook 权限。' })
      return
    }
    setSelectedNotebookEnabled(enabled)
    if (enabled) setSelectedKnowledgeBaseId('')
    if (!sessionId) return

    try {
      const res = await fetch(`/api/sessions/${sessionId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ notebook_enabled: enabled, kb: enabled ? '' : undefined }),
      })
      if (!res.ok) {
        throw new Error(`failed to update session source: HTTP ${res.status}`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setMessages((prev) => [...prev, { role: 'assistant', text: `Error: ${message}` }])
    }
  }, [pushStatus, running, selectedTutorId, sessionId, tutors])

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

  const handleApproval = (requestId: string, approved: boolean) => {
    send({ type: 'approval_response', request_id: requestId, approved })
    setPendingApproval(null)
    pushStatus({
      kind: 'thinking',
      label: approved ? 'Approval sent' : 'Tool denied',
      detail: 'Waiting for agent to continue',
    })
  }

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
      pendingQuizRef.current = undefined
      pendingQuizPlanRef.current = undefined
      pendingResearchPlanRef.current = undefined
      pendingResearchReportRef.current = undefined
      setLatestUsage(null)
      setRunning(false)
      await hydrateSession(id)
    }
    setView('chat')
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
      setSelectedTutorId(null)
      setMessages([])
      setStreamingText('')
      streamingRef.current = ''
      progressStreamingRef.current = ''
      setTraceEntries([])
      pendingCitationsRef.current = []
      pendingDeepSolveRef.current = []
      pendingNotebookEditProposalRef.current = undefined
      pendingQuizRef.current = undefined
      pendingQuizPlanRef.current = undefined
      pendingResearchPlanRef.current = undefined
      pendingResearchReportRef.current = undefined
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
      setSpaceFocusTarget(target)
      setView('notebook')
      pushStatus({
        kind: 'done',
        label: 'Opened source area',
        detail: sourceTargetDetail(target, reference),
      })
      return
    }

    if (target.type === 'quiz') {
      setSpaceFocusTarget(target)
      setView('space')
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

  const chatIsEmpty = view === 'chat' && messages.length === 0 && !streamingText && !sessionId
  const activeTutor = selectedTutorId ? tutors.find((item) => item.id === selectedTutorId) ?? null : null
  const t = (key: TranslationKey) => translate(llmSettings.language, key)

  return (
    <I18nProvider language={llmSettings.language}>
    <div className="app-shell flex h-screen overflow-hidden" data-theme={llmSettings.theme}>
      <Sidebar
        activeView={view}
        activeSessionId={view === 'chat' ? sessionId : null}
        collapsed={sidebarCollapsed}
        recentSessions={recentSessions}
        onNavigate={handleNavigate}
        onSelectSession={handleSelectSession}
        onRenameSession={handleRenameSession}
        onDeleteSession={handleDeleteSession}
        onTogglePinSession={handleTogglePinSession}
        onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {view === 'chat' && (
          <>
            <header className="flex items-center gap-4 bg-white px-6 py-3">
              <div>
                <h1 className="text-lg font-semibold text-gray-900">{t('chat.title')}</h1>
                <p className="text-xs text-gray-500">
                  {activeTutor?.name ?? '临时助手'}
                </p>
              </div>
              <div className="ml-auto">
                <BudgetPanel spent={budgetSpent} limit={llmSettings.budgetLimitUsd} warning={budgetWarning} />
              </div>
            </header>
            <div className="flex min-h-0 flex-1 overflow-hidden">
              <main className="min-h-0 min-w-0 flex-1 overflow-hidden">
                <ChatBox
                  sessionId={sessionId}
                  messages={messages}
                  streamingText={streamingText}
                  contextStats={contextStats}
                  capability={capability}
                  llmConfigs={llmSettings.llmConfigs}
                  activeLlmConfigId={selectedLlmConfigId}
                  knowledgeBases={activeTutor
                    ? knowledgeBases.filter((item) => activeTutor.resource_permissions.knowledge_base_ids.includes(item.id))
                    : knowledgeBases}
                  selectedKnowledgeBaseId={selectedKnowledgeBaseId}
                  selectedNotebookEnabled={selectedNotebookEnabled}
                  tutors={tutors}
                  selectedTutorId={selectedTutorId}
                  initialDraft={starterDraft}
                  onTutorSelect={handleTutorSelect}
                  onManageTutors={() => setView('tutor')}
                  onSend={handleSend}
                  onStop={handleStopGeneration}
                  onEditUserMessage={handleEditUserMessage}
                  onAskDeepSolveStep={handleAskDeepSolveStep}
                  onCapabilityChange={handleCapabilityChange}
                  onKnowledgeBaseChange={handleKnowledgeBaseChange}
                  onNotebookEnabledChange={handleNotebookEnabledChange}
                  onLlmConfigChange={handleLlmConfigChange}
                  notebookFolders={notebookFolders}
                  notebookEntryPaths={notebookEntryPaths}
                  notebookVault={notebookVault}
                  onSaveToNotebook={handleSaveToNotebook}
                  onOpenNotebookEntry={(entryId) => {
                    setSpaceFocusTarget({ type: 'notebook', entryId })
                    setView('notebook')
                  }}
                  onRegenerateResearch={handleRegenerateResearch}
                  onIngestResearchSources={handleIngestResearchSources}
                  onApplyNotebookEdit={handleApplyNotebookEdit}
                  onQuizAnswer={handleQuizAnswer}
                  onQuizFinish={handleQuizFinish}
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

        {view === 'tutor' && (
          <TutorPage
            tutors={tutors}
            modelConfigs={llmSettings.llmConfigs}
            knowledgeBases={knowledgeBases}
            onChanged={refreshTutors}
            onStartConversation={(tutorId) => {
              startNewChat()
              handleTutorSelect(tutorId)
            }}
          />
        )}

        {view === 'knowledge' && (
          <KnowledgePage settings={llmSettings} onChanged={refreshKnowledgeBases} focusTarget={knowledgeFocusTarget} />
        )}

        {view === 'notebook' && (
          <SpacePage mode="notebook" focusTarget={spaceFocusTarget} onSourceNavigate={handleSourceNavigate} />
        )}

        {view === 'space' && (
          <SpacePage
            focusTarget={spaceFocusTarget}
            onSourceNavigate={handleSourceNavigate}
            onStartQuiz={() => {
              startNewChat()
              setCapability('quiz')
              setStarterDraft({
                id: Date.now(),
                text: llmSettings.language === 'en-US'
                  ? 'Create a short quiz for me. First ask what topic or saved material I want to use.'
                  : '请为我生成一组简短测验，先询问我要使用的主题或已有材料。',
              })
            }}
          />
        )}

        {view === 'memory' && (
          <MemoryPage settings={llmSettings} onSourceNavigate={handleSourceNavigate} />
        )}

        {view === 'settings' && (
          <SettingsPage
            settings={llmSettings}
            activeTab={settingsTab}
            onTabChange={setSettingsTab}
            onChange={handleSettingsChange}
            onOpenOnboarding={openOnboarding}
            onGuideNavigate={handleGuideNavigate}
            onStartGuideTutor={startGuideTutor}
          />
        )}
      </div>

      <ApprovalDialog request={pendingApproval} onDecision={handleApproval} />
      {settingsHydrated && shouldShowOnboarding(llmSettings) && !onboardingOpen && (
        <OnboardingResumeButton onClick={openOnboarding} />
      )}
      {onboardingOpen && (
        <OnboardingDialog
          settings={llmSettings}
          tutors={tutors}
          knowledgeBaseCount={knowledgeBases.length}
          notebookVault={notebookVault}
          selectedTutorId={selectedTutorId}
          step={onboardingStep}
          onStepChange={setOnboardingStep}
          onTutorSelect={handleTutorSelect}
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
          onOpenNotebookSettings={() => {
            setOnboardingOpen(false)
            setSettingsTab('notebook')
            setView('settings')
          }}
          onOpenNotebook={() => {
            setOnboardingOpen(false)
            setView('notebook')
          }}
          onOpenMemory={() => {
            setOnboardingOpen(false)
            setView('memory')
          }}
          onManageTutors={() => {
            setOnboardingOpen(false)
            setView('tutor')
          }}
          onDismiss={pauseOnboarding}
          onComplete={completeOnboarding}
          onStartMode={startOnboardingMode}
        />
      )}
    </div>
    </I18nProvider>
  )
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

  if (kind === 'research_stage_start') {
    return {
      kind: 'thinking',
      label: typeof payload.title === 'string' ? payload.title : 'Planning research',
      detail: capability,
    }
  }

  if (kind === 'research_search') {
    return {
      kind: 'tool',
      label: 'Searching web',
      detail: capability,
    }
  }

  if (kind === 'research_read') {
    return {
      kind: 'tool',
      label: 'Reading source',
      detail: capability,
    }
  }

  if (kind === 'research_report_done') {
    return {
      kind: 'done',
      label: 'Research report ready',
      detail: capability,
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
  const isResearchToolResult = payload.kind === 'tool_result' && payload.tool === 'create_research_report'
  const isRagCitationEvent = payload.kind === 'rag_citations'
  const isResearchReportDone = payload.kind === 'research_report_done'
  if (!isRagToolResult && !isWebToolResult && !isResearchToolResult && !isRagCitationEvent && !isResearchReportDone) return []
  const details = isResearchReportDone ? payload : payload.details
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

function quizPlanFromTrace(payload: Record<string, unknown>): QuizPlan | undefined {
  if (payload.kind !== 'tool_result' || payload.tool !== 'propose_quiz_plan' || payload.ok === false) return undefined
  const details = payload.details
  if (!details || typeof details !== 'object') return undefined
  const item = details as Record<string, unknown>
  const title = typeof item.title === 'string' && item.title.trim() ? item.title : 'Quiz plan'
  const topic = typeof item.topic === 'string' && item.topic.trim() ? item.topic : 'selected material'
  const source = typeof item.source === 'string' && item.source.trim() ? item.source : 'current conversation'
  const difficulty = typeof item.difficulty === 'string' && item.difficulty.trim() ? item.difficulty : 'medium'
  const questionCount = typeof item.question_count === 'number' ? item.question_count : 5
  const notes = Array.isArray(item.notes)
    ? item.notes.filter((note): note is string => typeof note === 'string' && note.trim().length > 0)
    : []
  return {
    title,
    topic,
    source,
    difficulty,
    questionCount,
    notes,
  }
}

function researchPlanFromTrace(payload: Record<string, unknown>): ResearchPlan | undefined {
  if (payload.kind !== 'tool_result' || payload.tool !== 'propose_research_plan' || payload.ok === false) return undefined
  const details = payload.details
  if (!details || typeof details !== 'object') return undefined
  const item = details as Record<string, unknown>
  const title = typeof item.title === 'string' && item.title.trim() ? item.title : 'Research plan'
  const topic = typeof item.topic === 'string' && item.topic.trim() ? item.topic : 'selected topic'
  const scope = typeof item.scope === 'string' && item.scope.trim() ? item.scope : 'to be confirmed'
  const outputFormat = typeof item.output_format === 'string' && item.output_format.trim() ? item.output_format : 'Markdown report'
  const depth = typeof item.depth === 'string' && item.depth.trim() ? item.depth : 'standard'
  const timeRange = typeof item.time_range === 'string' && item.time_range.trim() ? item.time_range : 'not specified'
  return {
    title,
    topic,
    scope,
    outputFormat,
    depth,
    timeRange,
    sourcePreferences: stringListFromUnknown(item.source_preferences),
    useNotebook: item.use_notebook === true,
    useKnowledgeBase: item.use_knowledge_base === true,
    steps: stringListFromUnknown(item.steps),
    questions: stringListFromUnknown(item.questions),
  }
}

function researchReportFromTrace(payload: Record<string, unknown>): ResearchReportTraceData | undefined {
  return researchReportFromTracePayload(payload)
}

function stringListFromUnknown(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    : []
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

function attachRestoredQuizPlans(messages: Message[], traceEntries: TraceEntry[]): Message[] {
  const plans = traceEntries
    .map((entry) => quizPlanFromTrace(entry.payload))
    .filter((plan): plan is QuizPlan => Boolean(plan))
  if (plans.length === 0) return messages

  let planIndex = 0
  return messages.map((message) => {
    if (message.role !== 'assistant') return message
    if (message.quiz || message.quizPlan) return message
    const plan = plans[planIndex]
    planIndex += 1
    return plan ? { ...message, quizPlan: plan } : message
  })
}

function attachRestoredResearchPlans(messages: Message[], traceEntries: TraceEntry[]): Message[] {
  const plans = traceEntries
    .map((entry) => researchPlanFromTrace(entry.payload))
    .filter((plan): plan is ResearchPlan => Boolean(plan))
  if (plans.length === 0) return messages

  let planIndex = 0
  return messages.map((message) => {
    if (message.role !== 'assistant') return message
    if (message.quiz || message.quizPlan || message.researchPlan) return message
    const plan = plans[planIndex]
    planIndex += 1
    return plan ? { ...message, researchPlan: plan } : message
  })
}

async function attachRestoredQuizzes(messages: Message[], traceEntries: TraceEntry[]): Promise<Message[]> {
  return attachRestoredQuizzesToMessages(messages, traceEntries, async (id) => {
    try {
      const res = await fetch(`/api/quizzes/${encodeURIComponent(id)}`)
      const data = await safeJson(res)
      return res.ok ? data.quiz as QuizSession : null
    } catch {
      return null
    }
  })
}

function capabilityLabel(value: string): string {
  if (value === 'deep_solve') return 'Deep Solve'
  if (value === 'code_exec') return 'Code Exec'
  if (value === 'quiz') return 'Quiz'
  if (value === 'research') return 'Research'
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
  tutor: TutorSummary | null | undefined = undefined,
) {
  setRecentSessions((prev) => {
    const existing = prev.find((session) => session.id === id)
    const nextSession = {
      id,
      title,
      activeRun: existing?.activeRun ?? null,
      pinned: existing?.pinned ?? false,
      tutorId: tutor === undefined ? existing?.tutorId ?? null : tutor?.id ?? null,
      tutor: tutor === undefined ? existing?.tutor ?? null : tutor,
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


function researchSourceIngestText(source: SourceReference, markdown: string) {
  const url = source.target?.type === 'web' ? source.target.url : source.metadata?.url
  return [
    `# ${source.title || source.raw || 'Research source'}`,
    '',
    url ? `URL: ${url}` : '',
    source.score !== undefined && source.score !== null ? `Quality score: ${source.score.toFixed(2)}` : '',
    '',
    source.description || source.raw,
    '',
    '## Source report context',
    titleFromMarkdown(markdown),
  ].filter((line) => line !== '').join('\n')
}

async function pollIngestionJob(jobId: unknown) {
  if (typeof jobId !== 'string' || !jobId) {
    throw new Error('ingestion did not return a job id')
  }
  for (;;) {
    const res = await fetch(`/api/ingest-jobs/${encodeURIComponent(jobId)}`)
    const data = await safeJson(res)
    if (!res.ok) throw new Error(errorMessage(data, res.status))
    const job = data.job as { status?: string; error?: string | null; message?: string | null }
    if (job.status === 'done') return
    if (job.status === 'error') throw new Error(job.error || job.message || 'ingestion failed')
    await delay(500)
  }
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
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
  return value === 'chat' || value === 'deep_solve' || value === 'code_exec' || value === 'quiz' || value === 'research' || value === 'organize'
}

function sourceTargetDetail(target: SourceTarget, reference: SourceReference) {
  if (target.type === 'notebook') return `Notebook ${target.entryId}`
  if (target.type === 'quiz') return target.questionId ? `Quiz ${target.quizId}, question ${target.questionId}` : `Quiz ${target.quizId}`
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
