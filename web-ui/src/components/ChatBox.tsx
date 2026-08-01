import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { ChangeEvent, ReactNode, RefObject } from 'react'
import {
  AlertCircle,
  ArrowUp,
  AtSign,
  BookOpen,
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  Copy,
  Database,
  Edit3,
  FileText,
  FileQuestion,
  SearchCheck,
  Paperclip,
  Quote,
  RefreshCw,
  Circle,
  Square,
  X,
} from 'lucide-react'
import { chooseDesktopSavePath, isDesktopApp, writeClipboardText } from '../api'
import { appendMessageQuote, previousUserMessageIndex } from '../messageActions'
import {
  desktopDefaultSavePath,
  folderFromNotebookPath,
  loadLastNotebookSaveFolder,
  notebookFileNameFromTitle,
  notebookPath,
  notebookPathExists,
  relativeNotebookPath,
  saveLastNotebookSaveFolder,
  titleFromMarkdown,
} from '../notebookSave'
import type { NotebookVaultInfo, SaveToNotebookResult } from '../notebookSave'
import type { LlmModelConfig } from '../settings'
import type { QuizSession } from '../quizTypes'
import { useI18n } from '../i18n'
import { DeepSolveMessage, type DeepSolveTraceEntry } from './DeepSolveMessage'
import { MarkdownMessage, SourceReferences, sourceTargetFromRaw } from './MarkdownMessage'
import type { SourceReference, SourceTarget } from './MarkdownMessage'
import {
  loadChatScrollPosition,
  restoredScrollTop,
  saveChatScrollPosition,
  type ChatScrollPosition,
} from '../chatScrollPosition'
import { ResearchReportMessage, looksLikeResearchReport } from './ResearchReportMessage'
import { SaveNotebookDialog, SaveNotebookOutcomeDialog } from './SaveNotebookDialog'

type Capability = 'chat' | 'deep_solve' | 'code_exec' | 'quiz' | 'research' | 'organize'
type OpenMenu = 'knowledge' | 'space' | 'model' | null
type SpaceMentionFilter = 'all' | SpaceMention['type']

export interface SaveToNotebookOptions {
  folderPath?: string
  newFolderPath?: string
  filePath?: string
  entryType?: 'research_report' | 'chat_excerpt'
  title?: string
}

interface Message {
  role: 'user' | 'assistant' | 'status'
  text: string
  kind?: 'idle' | 'thinking' | 'tool' | 'done' | 'error'
  citations?: Citation[]
  deepSolve?: DeepSolveTraceEntry[]
  quiz?: QuizSession
  quizPlan?: QuizPlan
  researchPlan?: ResearchPlan
  researchTitle?: string
  researchUnavailable?: boolean
  notebookEditProposal?: NotebookEditProposal
  attachments?: ChatAttachment[]
  mentions?: SpaceMention[]
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

export interface ChatAttachment {
  id: string
  name: string
  size: number
  type: string
  text?: string
  error?: string
  truncated?: boolean
}

export interface SpaceMention {
  id: string
  type: 'notebook_entry' | 'quiz_session' | 'quiz_question'
  target_id?: string | null
  question_id?: string | null
  title: string
  preview?: string | null
  metadata?: Record<string, unknown>
}

export interface NotebookEditProposal {
  entryId: string
  entryTitle: string
  proposedTitle: string
  proposedMarkdown: string
  summary: string
  proposalKind?: 'edit' | 'links' | 'tags' | 'merge'
  suggestedLinks?: Array<{ text: string; target: string; reason?: string }>
  suggestedTags?: Array<{ tag: string; action: 'add' | 'keep' | 'remove'; reason?: string }>
  mergeSourceEntryIds?: string[]
  applied?: boolean
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

interface Props {
  sessionId: string | null
  messages: Message[]
  streamingText: string
  contextStats: ContextStats
  capability: Capability
  llmConfigs: LlmModelConfig[]
  activeLlmConfigId: string | null
  knowledgeBases: Array<{ id: string; name: string }>
  selectedKnowledgeBaseId: string
  selectedNotebookEnabled: boolean
  initialDraft?: { id: number; text: string } | null
  onSend: (text: string, attachments?: ChatAttachment[], mentions?: SpaceMention[]) => void
  onStop?: () => void
  onEditUserMessage?: (messageIndex: number, nextText: string) => void
  onAskDeepSolveStep?: (step: { id: string; title: string; summary?: string }) => void
  onCapabilityChange: (capability: Capability) => void
  onKnowledgeBaseChange: (id: string) => void
  onNotebookEnabledChange: (enabled: boolean) => void
  onLlmConfigChange: (id: string) => void
  notebookFolders?: string[]
  notebookEntryPaths?: string[]
  notebookVault?: NotebookVaultInfo | null
  onSaveToNotebook?: (markdown: string, options?: SaveToNotebookOptions) => Promise<SaveToNotebookResult>
  onOpenNotebookEntry?: (entryId: string) => void
  onRegenerateResearch?: (markdown: string) => void
  onIngestResearchSources?: (sources: SourceReference[], markdown: string) => Promise<void>
  onApplyNotebookEdit?: (proposal: NotebookEditProposal) => Promise<void>
  onQuizAnswer?: (quizId: string, questionId: string, selectedOptionId: string) => Promise<void>
  onQuizFinish?: (quizId: string) => Promise<void>
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  disabled: boolean
  running?: boolean
}

export interface ContextStats {
  usedTokens: number
  maxTokens: number
  source: 'provider' | 'estimate'
}

export function ChatBox({
  sessionId,
  messages,
  streamingText,
  contextStats,
  capability,
  llmConfigs,
  activeLlmConfigId,
  knowledgeBases,
  selectedKnowledgeBaseId,
  selectedNotebookEnabled,
  initialDraft,
  onSend,
  onStop,
  onEditUserMessage,
  onAskDeepSolveStep,
  onCapabilityChange,
  onKnowledgeBaseChange,
  onNotebookEnabledChange,
  onLlmConfigChange,
  notebookFolders = [],
  notebookEntryPaths = [],
  notebookVault,
  onSaveToNotebook,
  onOpenNotebookEntry,
  onRegenerateResearch,
  onIngestResearchSources,
  onApplyNotebookEdit,
  onQuizAnswer,
  onQuizFinish,
  onSourceNavigate,
  disabled,
  running = false,
}: Props) {
  const { t, language } = useI18n()
  const [input, setInput] = useState('')
  const [editingMessageIndex, setEditingMessageIndex] = useState<number | null>(null)
  const [editingMessageText, setEditingMessageText] = useState('')
  const [attachments, setAttachments] = useState<ChatAttachment[]>([])
  const [mentions, setMentions] = useState<SpaceMention[]>([])
  const [saveNotebookMarkdown, setSaveNotebookMarkdown] = useState<string | null>(null)
  const [saveNotebookFolder, setSaveNotebookFolder] = useState('')
  const [saveNotebookNewFolder, setSaveNotebookNewFolder] = useState('')
  const [saveNotebookFileName, setSaveNotebookFileName] = useState('')
  const [saveNotebookBusy, setSaveNotebookBusy] = useState(false)
  const [saveNotebookNative, setSaveNotebookNative] = useState(false)
  const [saveNotebookEntryType, setSaveNotebookEntryType] = useState<'research_report' | 'chat_excerpt'>('chat_excerpt')
  const [saveNotebookResult, setSaveNotebookResult] = useState<SaveToNotebookResult | null>(null)
  const [saveNotebookError, setSaveNotebookError] = useState('')
  const [copiedMessageIndex, setCopiedMessageIndex] = useState<number | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const composerInputRef = useRef<HTMLTextAreaElement>(null)
  const consumedDraftIdRef = useRef<number | null>(null)
  const copyFeedbackTimerRef = useRef<number | null>(null)
  const scrollSaveTimerRef = useRef<number | null>(null)
  const latestScrollPositionRef = useRef<{ sessionId: string; position: ChatScrollPosition } | null>(null)
  const pendingScrollRestoreRef = useRef<{ sessionId: string; position: ChatScrollPosition | null } | null>(null)
  const shouldStickToBottomRef = useRef(true)
  const empty = messages.length === 0 && !streamingText

  useEffect(() => {
    if (!initialDraft || consumedDraftIdRef.current === initialDraft.id) return
    consumedDraftIdRef.current = initialDraft.id
    setInput(initialDraft.text)
    window.requestAnimationFrame(() => {
      const composer = composerInputRef.current
      if (!composer) return
      composer.focus()
      composer.setSelectionRange(composer.value.length, composer.value.length)
    })
  }, [initialDraft])

  const handleSend = () => {
    const readyAttachments = attachments.filter((attachment) => !attachment.error)
    if ((!input.trim() && readyAttachments.length === 0 && mentions.length === 0) || disabled || running) return
    shouldStickToBottomRef.current = true
    onSend(input.trim(), readyAttachments, mentions)
    setInput('')
    setAttachments([])
    setMentions([])
  }

  const startEditUserMessage = (index: number, text: string) => {
    if (running) return
    setEditingMessageIndex(index)
    setEditingMessageText(text)
  }

  const cancelEditUserMessage = () => {
    setEditingMessageIndex(null)
    setEditingMessageText('')
  }

  const submitEditUserMessage = () => {
    if (editingMessageIndex === null || !editingMessageText.trim() || !onEditUserMessage || running) return
    onEditUserMessage(editingMessageIndex, editingMessageText.trim())
    cancelEditUserMessage()
  }

  const copyMessage = async (index: number, text: string) => {
    const copiedNatively = await writeClipboardText(text).catch(() => false)
    if (!copiedNatively) copyTextWithDocumentFallback(text)
    setCopiedMessageIndex(index)
    if (copyFeedbackTimerRef.current !== null) window.clearTimeout(copyFeedbackTimerRef.current)
    copyFeedbackTimerRef.current = window.setTimeout(() => setCopiedMessageIndex(null), 1600)
  }

  const quoteMessage = (role: 'user' | 'assistant', text: string) => {
    setInput((current) => appendMessageQuote(current, role, text))
    window.requestAnimationFrame(() => {
      const composer = composerInputRef.current
      if (!composer) return
      composer.focus()
      composer.setSelectionRange(composer.value.length, composer.value.length)
    })
  }

  const regenerateAssistantMessage = (messageIndex: number) => {
    if (!onEditUserMessage || running) return
    const userMessageIndex = previousUserMessageIndex(messages, messageIndex)
    const userMessage = messages[userMessageIndex]
    if (userMessageIndex < 0 || userMessage?.role !== 'user' || !userMessage.text.trim()) return
    onEditUserMessage(userMessageIndex, userMessage.text)
  }

  const focusMessageSources = (messageIndex: number) => {
    const sourceSurface = document.getElementById(`message-sources-${messageIndex}`)
    const toggle = sourceSurface?.querySelector<HTMLButtonElement>('button')
    toggle?.focus()
    toggle?.click()
  }

  const handleAddAttachments = (items: ChatAttachment[]) => {
    setAttachments((current) => [...current, ...items])
  }

  const handleRemoveAttachment = (id: string) => {
    setAttachments((current) => current.filter((attachment) => attachment.id !== id))
  }

  const handleAddMention = (mention: SpaceMention) => {
    setMentions((current) => current.some((item) => item.id === mention.id) ? current : [...current, mention])
  }

  const handleRemoveMention = (id: string) => {
    setMentions((current) => current.filter((mention) => mention.id !== id))
  }

  const openSaveNotebookDialog = async (
    markdown: string,
    entryType: 'research_report' | 'chat_excerpt' = 'chat_excerpt',
    structuredTitle?: string,
  ) => {
    const title = structuredTitle?.trim() || titleFromMarkdown(markdown)
    const fileName = notebookFileNameFromTitle(title)
    const lastFolder = loadLastNotebookSaveFolder(notebookFolders)
    setSaveNotebookResult(null)
    setSaveNotebookError('')
    setSaveNotebookFileName(fileName)
    setSaveNotebookEntryType(entryType)
    setSaveNotebookFolder(lastFolder)
    setSaveNotebookNewFolder('')
    if (notebookVault?.external && await isDesktopApp().catch(() => false)) {
      setSaveNotebookNative(true)
      await saveToExternalVault(markdown, fileName, lastFolder, entryType, title)
      return
    }
    setSaveNotebookNative(false)
    setSaveNotebookMarkdown(markdown)
  }

  const saveToExternalVault = async (
    markdown: string,
    fileName: string,
    folderPath: string,
    entryType: 'research_report' | 'chat_excerpt',
    title: string,
  ) => {
    if (!onSaveToNotebook || !notebookVault) return
    setSaveNotebookMarkdown(markdown)
    try {
      const selectedPath = await chooseDesktopSavePath(
        '保存到 Notebook Vault',
        desktopDefaultSavePath(notebookVault.root, folderPath, fileName),
      )
      if (!selectedPath) {
        closeSaveNotebookDialog()
        return
      }
      const relativePath = relativeNotebookPath(notebookVault.root, selectedPath)
      const selectedTitle = relativePath.split('/').pop()?.replace(/\.md$/i, '').trim() || title
      setSaveNotebookFolder(folderFromNotebookPath(relativePath))
      setSaveNotebookFileName(relativePath.split('/').pop() ?? fileName)
      if (notebookPathExists(relativePath, notebookEntryPaths)) {
        throw new Error('该位置已经存在同名 Notebook 笔记，请选择其他文件名。')
      }
      setSaveNotebookBusy(true)
      const result = await onSaveToNotebook(markdown, { filePath: relativePath, entryType, title: selectedTitle })
      saveLastNotebookSaveFolder(folderFromNotebookPath(result.path))
      setSaveNotebookResult(result)
    } catch (error) {
      setSaveNotebookError(error instanceof Error ? error.message : String(error))
    } finally {
      setSaveNotebookBusy(false)
    }
  }

  const closeSaveNotebookDialog = () => {
    if (saveNotebookBusy) return
    setSaveNotebookMarkdown(null)
    setSaveNotebookFolder('')
    setSaveNotebookNewFolder('')
    setSaveNotebookFileName('')
    setSaveNotebookNative(false)
    setSaveNotebookEntryType('chat_excerpt')
    setSaveNotebookResult(null)
    setSaveNotebookError('')
  }

  const submitSaveNotebook = async () => {
    if (!onSaveToNotebook || !saveNotebookMarkdown || saveNotebookBusy) return
    setSaveNotebookBusy(true)
    try {
      const result = await onSaveToNotebook(saveNotebookMarkdown, {
        folderPath: saveNotebookFolder || undefined,
        newFolderPath: saveNotebookNewFolder.trim() || undefined,
        filePath: notebookPath(saveNotebookNewFolder || saveNotebookFolder, saveNotebookFileName),
        entryType: saveNotebookEntryType,
        title: saveNotebookFileName.replace(/\.md$/i, ''),
      })
      saveLastNotebookSaveFolder(folderFromNotebookPath(result.path))
      setSaveNotebookResult(result)
      setSaveNotebookError('')
    } catch (error) {
      setSaveNotebookError(error instanceof Error ? error.message : String(error))
    } finally {
      setSaveNotebookBusy(false)
    }
  }

  const handleScroll = () => {
    const el = scrollRef.current
    if (!el) return

    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    shouldStickToBottomRef.current = distanceFromBottom < 80
    if (!sessionId) return

    latestScrollPositionRef.current = {
      sessionId,
      position: {
        scrollTop: el.scrollTop,
        atBottom: shouldStickToBottomRef.current,
      },
    }
    if (scrollSaveTimerRef.current !== null) window.clearTimeout(scrollSaveTimerRef.current)
    scrollSaveTimerRef.current = window.setTimeout(flushScrollPosition, 120)
  }

  const flushScrollPosition = () => {
    if (scrollSaveTimerRef.current !== null) {
      window.clearTimeout(scrollSaveTimerRef.current)
      scrollSaveTimerRef.current = null
    }
    const latest = latestScrollPositionRef.current
    if (latest) saveChatScrollPosition(latest.sessionId, latest.position)
  }

  useLayoutEffect(() => {
    if (!sessionId) {
      pendingScrollRestoreRef.current = null
      shouldStickToBottomRef.current = true
      return
    }
    const position = loadChatScrollPosition(sessionId)
    pendingScrollRestoreRef.current = { sessionId, position }
    shouldStickToBottomRef.current = position?.atBottom ?? true
  }, [sessionId])

  useLayoutEffect(() => {
    const pending = pendingScrollRestoreRef.current
    const el = scrollRef.current
    if (!pending || pending.sessionId !== sessionId || !el) return
    if (messages.length === 0 && !streamingText) return

    el.scrollTop = restoredScrollTop(pending.position, el.scrollHeight, el.clientHeight)
    shouldStickToBottomRef.current = pending.position?.atBottom ?? true
    pendingScrollRestoreRef.current = null
  }, [messages.length, sessionId, streamingText])

  useEffect(() => {
    const el = scrollRef.current
    if (!el || !shouldStickToBottomRef.current) return

    el.scrollTop = el.scrollHeight
  }, [messages, streamingText])

  useEffect(() => () => {
    if (copyFeedbackTimerRef.current !== null) window.clearTimeout(copyFeedbackTimerRef.current)
    flushScrollPosition()
  }, [])

  return (
    <div className="chat-canvas flex h-full min-h-0 flex-col overflow-hidden">
      {saveNotebookMarkdown && (
        saveNotebookResult || saveNotebookNative ? (
          <SaveNotebookOutcomeDialog
            result={saveNotebookResult}
            error={saveNotebookError}
            busy={saveNotebookBusy}
            onClose={closeSaveNotebookDialog}
            onOpen={() => {
              if (saveNotebookResult) onOpenNotebookEntry?.(saveNotebookResult.entryId)
              closeSaveNotebookDialog()
            }}
            onRetry={() => void saveToExternalVault(
              saveNotebookMarkdown,
              saveNotebookFileName,
              saveNotebookFolder,
              saveNotebookEntryType,
              saveNotebookFileName.replace(/\.md$/i, ''),
            )}
          />
        ) : (
          <SaveNotebookDialog
            folders={notebookFolders}
            entryPaths={notebookEntryPaths}
            selectedFolder={saveNotebookFolder}
            newFolder={saveNotebookNewFolder}
            fileName={saveNotebookFileName}
            busy={saveNotebookBusy}
            error={saveNotebookError}
            onSelectedFolderChange={(folder) => {
              setSaveNotebookFolder(folder)
              setSaveNotebookNewFolder('')
            }}
            onNewFolderChange={setSaveNotebookNewFolder}
            onFileNameChange={setSaveNotebookFileName}
            onCancel={closeSaveNotebookDialog}
            onSave={() => void submitSaveNotebook()}
          />
        )
      )}
      {empty ? (
        <div className="chat-scroll-pane flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-6 pb-20">
          <div className="w-full max-w-4xl">
            <div className="mb-7 text-center">
              <h2 className="text-3xl font-semibold text-gray-950">{t('chat.empty.title')}</h2>
              <p className="mt-2 text-sm text-gray-500">
                {capability === 'research'
                  ? language === 'en-US'
                    ? 'Describe what you want to investigate. The agent will clarify scope before starting the research workflow.'
                    : '描述你想调研的主题，Agent 会先确认范围，再启动详细研究 workflow。'
                  : capability === 'quiz'
                    ? language === 'en-US'
                      ? 'Name a topic or reference saved material, then generate a quiz in this conversation.'
                      : '输入一个主题或引用已有材料，在当前会话中生成测验。'
                    : t('chat.empty.description')}
              </p>
            </div>
            <Composer
              inputRef={composerInputRef}
              input={input}
              setInput={setInput}
              capability={capability}
              llmConfigs={llmConfigs}
              activeLlmConfigId={activeLlmConfigId}
              knowledgeBases={knowledgeBases}
              selectedKnowledgeBaseId={selectedKnowledgeBaseId}
              selectedNotebookEnabled={selectedNotebookEnabled}
              onCapabilityChange={onCapabilityChange}
              onKnowledgeBaseChange={onKnowledgeBaseChange}
              onNotebookEnabledChange={onNotebookEnabledChange}
              onLlmConfigChange={onLlmConfigChange}
              onSend={handleSend}
              onStop={onStop}
              attachments={attachments}
              onAddAttachments={handleAddAttachments}
              onRemoveAttachment={handleRemoveAttachment}
              mentions={mentions}
              onAddMention={handleAddMention}
              onRemoveMention={handleRemoveMention}
              disabled={disabled}
              running={running}
              variant="center"
            />
          </div>
        </div>
      ) : (
        <>
          <ContextCapacity stats={contextStats} />
          <div ref={scrollRef} onScroll={handleScroll} className="chat-scroll-pane min-h-0 flex-1 overflow-y-auto p-4">
            <div className="mx-auto w-full max-w-6xl space-y-3">
            {messages.map((msg, i) => {
              const structuredAssistant = isStructuredAssistantMessage(msg, capability)
              const previousUserIndex = msg.role === 'assistant' ? previousUserMessageIndex(messages, i) : -1
              return (
              <div key={i} className={messageClassName(msg, structuredAssistant)}>
                {msg.role === 'status' ? (
                  <div className="flex items-center gap-2 text-sm text-gray-600">
                    {(msg.kind === 'thinking' || msg.kind === 'tool') && (
                      <span className="h-2 w-2 animate-pulse rounded-full bg-current" />
                    )}
                    <span>{msg.text}</span>
                  </div>
                ) : msg.role === 'assistant' ? (
                  msg.quiz ? (
                    <ChatQuizCard
                      quiz={msg.quiz}
                      onAnswer={onQuizAnswer}
                      onFinish={onQuizFinish}
                      onSourceNavigate={onSourceNavigate}
                    />
                  ) : msg.quizPlan ? (
                    <QuizPlanCard plan={msg.quizPlan} text={msg.text} />
                  ) : msg.researchPlan ? (
                    <ResearchPlanCard
                      plan={msg.researchPlan}
                      text={msg.text}
                      onStart={() => {
                        onSend(startResearchPrompt(msg.researchPlan!), [], [])
                      }}
                    />
                  ) : msg.deepSolve && msg.deepSolve.length > 0 ? (
                    <DeepSolveMessage
                      text={msg.text}
                      events={msg.deepSolve}
                      citations={msg.citations}
                      citationList={(citations) => <CitationList citations={citations} onSourceNavigate={onSourceNavigate} />}
                      onAskStep={onAskDeepSolveStep}
                    />
                  ) : capability === 'research' && (looksLikeResearchReport(msg.text) || msg.researchUnavailable) ? (
                    <ResearchReportMessage
                      text={msg.text}
                      reportTitle={msg.researchTitle}
                      unavailable={msg.researchUnavailable}
                      sources={(msg.citations ?? []).map(citationToResearchSourceReference)}
                      onSaveToNotebook={(markdown, title) => void openSaveNotebookDialog(markdown, 'research_report', title)}
                      onRegenerate={onRegenerateResearch}
                      onIngestSources={onIngestResearchSources}
                      onSourceNavigate={onSourceNavigate}
                    />
                  ) : (
                    <>
                      <div className="assistant-message-surface">
                        <MarkdownMessage text={msg.text} onSourceNavigate={onSourceNavigate} />
                        {msg.citations && msg.citations.length > 0 && (
                          <CitationList
                            id={`message-sources-${i}`}
                            citations={msg.citations}
                            onSourceNavigate={onSourceNavigate}
                          />
                        )}
                      </div>
                      {msg.notebookEditProposal && onApplyNotebookEdit && (
                        <NotebookEditProposalCard
                          proposal={msg.notebookEditProposal}
                          onApply={onApplyNotebookEdit}
                        />
                      )}
                      <MessageActionToolbar
                        align="left"
                        copied={copiedMessageIndex === i}
                        onCopy={() => void copyMessage(i, msg.text)}
                        onQuote={() => quoteMessage('assistant', msg.text)}
                        onSaveToNotebook={
                          capability === 'research' && msg.text.trim() && onSaveToNotebook
                            ? () => void openSaveNotebookDialog(msg.text)
                            : undefined
                        }
                        onRegenerate={
                          previousUserIndex >= 0 && onEditUserMessage && !running
                            ? () => regenerateAssistantMessage(i)
                            : undefined
                        }
                        sourceCount={msg.citations?.length ?? 0}
                        onShowSources={msg.citations?.length ? () => focusMessageSources(i) : undefined}
                      />
                    </>
                  )
                ) : (
                  <>
                    {editingMessageIndex === i ? (
                      <div className="user-message-surface w-full max-w-[85%] space-y-2 rounded-lg p-3">
                        <textarea
                          className="min-h-24 w-full resize-y rounded-lg border border-blue-200 bg-white px-3 py-2 text-sm text-gray-900 outline-none focus:border-blue-400"
                          value={editingMessageText}
                          onChange={(event) => setEditingMessageText(event.target.value)}
                          autoFocus
                        />
                        <div className="flex justify-end gap-2">
                          <button
                            className="rounded-md border border-gray-200 bg-white px-3 py-1.5 text-xs font-medium text-gray-600 hover:bg-gray-50"
                            type="button"
                            onClick={cancelEditUserMessage}
                          >
                            取消
                          </button>
                          <button
                            className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-blue-700 disabled:bg-gray-200"
                            type="button"
                            disabled={!editingMessageText.trim()}
                            onClick={submitEditUserMessage}
                          >
                            Regenerate
                          </button>
                        </div>
                      </div>
                    ) : (
                      <div className="user-message-surface w-fit max-w-[85%] rounded-lg px-4 py-3">
                        <pre className="whitespace-pre-wrap font-sans text-sm">{msg.text}</pre>
                        {msg.attachments && msg.attachments.length > 0 && (
                          <AttachmentSummary attachments={msg.attachments} />
                        )}
                        {msg.mentions && msg.mentions.length > 0 && (
                          <MentionSummary mentions={msg.mentions} />
                        )}
                      </div>
                    )}
                    {editingMessageIndex !== i && (
                      <MessageActionToolbar
                        align="right"
                        copied={copiedMessageIndex === i}
                        onCopy={() => void copyMessage(i, msg.text)}
                        onQuote={() => quoteMessage('user', msg.text)}
                        onEdit={onEditUserMessage && !running ? () => startEditUserMessage(i, msg.text) : undefined}
                      />
                    )}
                  </>
                )}
              </div>
              )
            })}
            {streamingText && (
              <div className="w-full min-w-0 py-2 text-gray-900" aria-live="polite">
                <div className="assistant-message-surface">
                  <MarkdownMessage text={streamingText} onSourceNavigate={onSourceNavigate} />
                  <span className="inline-block h-4 w-0.5 animate-pulse bg-gray-700 align-text-bottom" />
                </div>
              </div>
            )}
            </div>
          </div>
          <div className="composer-dock p-4">
            <Composer
              inputRef={composerInputRef}
              input={input}
              setInput={setInput}
              capability={capability}
              llmConfigs={llmConfigs}
              activeLlmConfigId={activeLlmConfigId}
              knowledgeBases={knowledgeBases}
              selectedKnowledgeBaseId={selectedKnowledgeBaseId}
              selectedNotebookEnabled={selectedNotebookEnabled}
              onCapabilityChange={onCapabilityChange}
              onKnowledgeBaseChange={onKnowledgeBaseChange}
              onNotebookEnabledChange={onNotebookEnabledChange}
              onLlmConfigChange={onLlmConfigChange}
              onSend={handleSend}
              onStop={onStop}
              attachments={attachments}
              onAddAttachments={handleAddAttachments}
              onRemoveAttachment={handleRemoveAttachment}
              mentions={mentions}
              onAddMention={handleAddMention}
              onRemoveMention={handleRemoveMention}
              disabled={disabled}
              running={running}
              variant="bottom"
            />
          </div>
        </>
      )}
    </div>
  )
}

function ContextCapacity({ stats }: { stats: ContextStats }) {
  const maxTokens = Math.max(1, stats.maxTokens)
  const usedTokens = Math.max(0, stats.usedTokens)
  const percent = Math.min(100, Math.round((usedTokens / maxTokens) * 100))
  const tone =
    percent >= 90
      ? 'bg-red-500'
      : percent >= 75
        ? 'bg-amber-500'
        : 'bg-blue-600'

  return (
    <div className="border-b border-blue-50 bg-white px-5 py-2">
      <div className="flex items-center gap-3 text-xs text-gray-500">
        <span className="font-medium text-gray-700">Context capacity</span>
        <div className="h-1.5 w-36 overflow-hidden rounded-full bg-gray-100">
          <div className={`h-full rounded-full ${tone}`} style={{ width: `${percent}%` }} />
        </div>
        <span>
          {formatTokenCount(usedTokens)} / {formatTokenCount(maxTokens)}
        </span>
        <span className="text-gray-400">{percent}%</span>
        <span className="text-gray-400">{stats.source === 'provider' ? '上次请求' : '估算'}</span>
      </div>
    </div>
  )
}

function formatTokenCount(value: number) {
  if (value >= 1000) return `${(value / 1000).toFixed(value >= 10000 ? 0 : 1)}k`
  return String(value)
}

function NotebookEditProposalCard({
  proposal,
  onApply,
}: {
  proposal: NotebookEditProposal
  onApply: (proposal: NotebookEditProposal) => Promise<void>
}) {
  return (
    <div className="mt-3 overflow-hidden rounded-lg border border-blue-100 bg-white">
      <div className="flex items-start justify-between gap-3 border-b border-blue-50 px-4 py-3">
        <div>
          <div className="text-sm font-semibold text-gray-900">{notebookProposalTitle(proposal)}</div>
          <div className="mt-1 text-xs text-gray-500">{proposal.entryTitle}</div>
        </div>
        {proposal.applied ? (
          <span className="inline-flex items-center gap-1 rounded-full bg-green-50 px-2 py-1 text-xs font-medium text-green-700">
            <CheckCircle2 size={14} />
            Applied
          </span>
        ) : (
          <button
            className="inline-flex h-8 items-center gap-2 rounded-md bg-blue-600 px-3 text-xs font-semibold text-white hover:bg-blue-700"
            type="button"
            onClick={() => {
              void onApply(proposal)
            }}
          >
            <CheckCircle2 size={15} />
            Apply
          </button>
        )}
      </div>
      <div className="space-y-3 px-4 py-3">
        <p className="text-sm text-gray-700">{proposal.summary}</p>
        {proposal.suggestedLinks && proposal.suggestedLinks.length > 0 && (
          <ProposalDetailList
            title="Suggested links"
            items={proposal.suggestedLinks.map((link) =>
              `${link.text} -> [[${link.target}]]${link.reason ? ` - ${link.reason}` : ''}`,
            )}
          />
        )}
        {proposal.suggestedTags && proposal.suggestedTags.length > 0 && (
          <ProposalDetailList
            title="Suggested tags"
            items={proposal.suggestedTags.map((tag) =>
              `${tag.action}: #${tag.tag.replace(/^#/, '')}${tag.reason ? ` - ${tag.reason}` : ''}`,
            )}
          />
        )}
        {proposal.mergeSourceEntryIds && proposal.mergeSourceEntryIds.length > 0 && (
          <ProposalDetailList
            title="Merge sources"
            items={proposal.mergeSourceEntryIds.map((id) => `Notebook entry ${id}`)}
          />
        )}
        <div className="rounded-md bg-gray-50 px-3 py-2 text-xs text-gray-600">
          <span className="font-medium text-gray-900">New title:</span> {proposal.proposedTitle}
        </div>
        <details className="rounded-md border border-gray-100 bg-gray-50 px-3 py-2">
          <summary className="cursor-pointer text-xs font-medium text-gray-700">Preview Markdown</summary>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap text-xs text-gray-700">{proposal.proposedMarkdown}</pre>
        </details>
      </div>
    </div>
  )
}

function notebookProposalTitle(proposal: NotebookEditProposal) {
  if (proposal.proposalKind === 'links') return 'Notebook link proposal'
  if (proposal.proposalKind === 'tags') return 'Notebook tag proposal'
  if (proposal.proposalKind === 'merge') return 'Notebook merge proposal'
  return 'Notebook edit proposal'
}

function ProposalDetailList({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="rounded-md border border-blue-50 bg-blue-50/40 px-3 py-2">
      <div className="text-xs font-semibold text-blue-900">{title}</div>
      <ul className="mt-1 space-y-1 text-xs leading-5 text-gray-700">
        {items.map((item, index) => (
          <li key={`${title}-${index}`}>{item}</li>
        ))}
      </ul>
    </div>
  )
}

function CitationList({
  id,
  citations,
  onSourceNavigate,
}: {
  id?: string
  citations: Citation[]
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const rawId = useId()
  const hasWeb = citations.some((citation) => citation.kind === 'web' || citation.url)
  const references = citations.map(citationToSourceReference)
  return (
    <div id={id} className="mt-3 border-t border-gray-200 pt-3" data-source-kind={hasWeb ? 'web' : 'rag'}>
      <div className="mb-2 text-xs font-medium text-gray-500">{hasWeb ? '网页来源' : '引用来源'}</div>
      <SourceReferences
        id={`chat-citations-${rawId.replace(/[^a-zA-Z0-9_-]/g, '')}`}
        references={references}
        onNavigate={onSourceNavigate}
      />
    </div>
  )
}

function citationToSourceReference(citation: Citation, index: number): SourceReference {
  const raw = citation.url || citationRawTarget(citation)
  const target = sourceTargetFromRaw(raw)
  return {
    id: `${citation.index || index + 1}:${raw}`,
    label: String(citation.index || index + 1),
    raw,
    surface: citation.kind === 'web' || citation.url || target?.type === 'web' ? 'web' : target?.type === 'kb' ? 'kb' : 'unknown',
    title: citation.title || citation.source,
    description: citation.text,
    score: citation.score,
    metadata: {
      documentName: citation.kind === 'rag' ? citation.title || citation.source : undefined,
      documentId: citation.documentId,
      chunkId: citation.chunkId,
      page: citation.page,
      url: citation.url,
      missingReason: target ? undefined : 'No navigable source id was provided by the tool result.',
    },
    target,
  }
}

function citationToResearchSourceReference(citation: Citation, index: number): SourceReference {
  const reference = citationToSourceReference(citation, index)
  return {
    ...reference,
    description: truncateSourceDescription(reference.description),
  }
}

function truncateSourceDescription(value?: string) {
  if (!value) return value
  const normalized = value.replace(/\s+/g, ' ').trim()
  return normalized.length > 420 ? `${normalized.slice(0, 420)}...` : normalized
}

function citationRawTarget(citation: Citation) {
  if (citation.kb && citation.documentId) {
    return ['kb', citation.kb, citation.documentId, citation.chunkId].filter(Boolean).join(':')
  }
  return citation.rawSource || citation.source
}

function QuizPlanCard({ plan, text }: { plan: QuizPlan; text: string }) {
  return (
    <div className="space-y-3 rounded-lg border border-blue-100 bg-white p-4">
      {text.trim() && <MarkdownMessage text={text} />}
      <div className="rounded-lg border border-blue-100 bg-blue-50/40 p-3">
        <div className="flex items-center gap-2 text-sm font-semibold text-blue-800">
          <FileQuestion size={17} />
          Quiz plan
        </div>
        <div className="mt-3 grid gap-2 text-sm text-gray-700 sm:grid-cols-2">
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Title</span>
            <p className="font-medium text-gray-950">{plan.title}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Topic</span>
            <p>{plan.topic}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Source</span>
            <p>{plan.source}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Settings</span>
            <p>
              {plan.questionCount} questions · {plan.difficulty}
            </p>
          </div>
        </div>
        {plan.notes.length > 0 && (
          <ul className="mt-3 list-disc space-y-1 pl-5 text-sm text-gray-600">
            {plan.notes.map((note, index) => (
              <li key={`${index}:${note}`}>{note}</li>
            ))}
          </ul>
        )}
        <p className="mt-3 text-xs text-gray-500">Reply with confirmation or changes before generating the quiz.</p>
      </div>
    </div>
  )
}

function ResearchPlanCard({
  plan,
  text,
  onStart,
}: {
  plan: ResearchPlan
  text: string
  onStart: () => void
}) {
  return (
    <div className="space-y-3 rounded-lg border border-blue-100 bg-white p-4">
      {text.trim() && <MarkdownMessage text={text} />}
      <div className="rounded-lg border border-blue-100 bg-blue-50/40 p-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-blue-800">
            <SearchCheck size={17} />
            Research plan
          </div>
          <button
            className="inline-flex h-8 items-center gap-2 rounded-md bg-blue-600 px-3 text-xs font-semibold text-white hover:bg-blue-700"
            type="button"
            onClick={onStart}
          >
            <SearchCheck size={15} />
            Start detailed research
          </button>
        </div>
        <div className="mt-3 grid gap-2 text-sm text-gray-700 sm:grid-cols-2">
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Title</span>
            <p className="font-medium text-gray-950">{plan.title}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Topic</span>
            <p>{plan.topic}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Scope</span>
            <p>{plan.scope}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Output</span>
            <p>
              {plan.outputFormat} · {plan.depth}
            </p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Time range</span>
            <p>{plan.timeRange}</p>
          </div>
          <div>
            <span className="text-xs font-medium uppercase text-gray-400">Context</span>
            <p>
              {[
                plan.useNotebook ? 'Notebook' : null,
                plan.useKnowledgeBase ? 'Knowledge Base' : null,
              ].filter(Boolean).join(' + ') || 'Conversation and web sources'}
            </p>
          </div>
        </div>
        {plan.sourcePreferences.length > 0 && (
          <div className="mt-3">
            <span className="text-xs font-medium uppercase text-gray-400">Sources</span>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {plan.sourcePreferences.map((source) => (
                <span key={source} className="rounded-full bg-white px-2 py-1 text-xs text-gray-600">
                  {source}
                </span>
              ))}
            </div>
          </div>
        )}
        {plan.steps.length > 0 && (
          <ol className="mt-3 list-decimal space-y-1 pl-5 text-sm text-gray-600">
            {plan.steps.map((step, index) => (
              <li key={`${index}:${step}`}>{step}</li>
            ))}
          </ol>
        )}
        {plan.questions.length > 0 && (
          <ul className="mt-3 list-disc space-y-1 pl-5 text-sm text-gray-600">
            {plan.questions.map((question, index) => (
              <li key={`${index}:${question}`}>{question}</li>
            ))}
          </ul>
        )}
        <p className="mt-3 text-xs text-gray-500">Confirm, revise, or start the detailed research workflow.</p>
      </div>
    </div>
  )
}

function startResearchPrompt(plan: ResearchPlan) {
  return [
    'Start the detailed research workflow for this confirmed plan.',
    `Title: ${plan.title}`,
    `Topic: ${plan.topic}`,
    `Scope: ${plan.scope}`,
    `Output format: ${plan.outputFormat}`,
    `Depth: ${plan.depth}`,
    `Time range: ${plan.timeRange}`,
    plan.sourcePreferences.length > 0 ? `Source preferences: ${plan.sourcePreferences.join(', ')}` : '',
    plan.useNotebook ? 'Use Notebook context if relevant.' : '',
    plan.useKnowledgeBase ? 'Use the selected Knowledge Base if relevant.' : '',
    'Search, read sources, synthesize the report, verify citations, and return the final Markdown report.',
  ].filter(Boolean).join('\n')
}

function ChatQuizCard({
  quiz,
  onAnswer,
  onFinish,
  onSourceNavigate,
}: {
  quiz: QuizSession
  onAnswer?: (quizId: string, questionId: string, selectedOptionId: string) => Promise<void>
  onFinish?: (quizId: string) => Promise<void>
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const [currentIndex, setCurrentIndex] = useState(0)
  const [selectedOptionId, setSelectedOptionId] = useState('')
  const [busy, setBusy] = useState(false)
  const question = quiz.questions[currentIndex] ?? null
  const answer = question ? quiz.answers.find((item) => item.question_id === question.id) ?? null : null
  const score = quiz.score ?? { correct: 0, total: quiz.questions.length }

  useEffect(() => {
    setSelectedOptionId(answer?.selected_option_id ?? '')
  }, [answer?.selected_option_id, question?.id])

  if (!question) {
    return (
      <div className="rounded-lg border border-blue-100 bg-white p-4">
        <div className="flex items-center gap-2 text-sm font-semibold text-blue-800">
          <FileQuestion size={18} />
          Quiz
        </div>
        <p className="mt-3 text-sm text-gray-600">This quiz does not have generated questions yet.</p>
      </div>
    )
  }

  const submit = async () => {
    if (!selectedOptionId || answer || !onAnswer || busy) return
    setBusy(true)
    try {
      await onAnswer(quiz.id, question.id, selectedOptionId)
    } finally {
      setBusy(false)
    }
  }

  const finish = async () => {
    if (!onFinish || busy || quiz.status === 'finished') return
    setBusy(true)
    try {
      await onFinish(quiz.id)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-4 rounded-lg border border-blue-100 bg-white p-4">
      <div className="flex items-start gap-3">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
          <FileQuestion size={19} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-base font-semibold text-gray-950">{quiz.title || 'Quiz'}</h3>
            <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium text-blue-700">
              {quiz.status}
            </span>
            {quiz.verification && (
              <span
                className={`rounded-full px-2 py-0.5 text-xs font-medium ${
                  quiz.verification.status === 'verified'
                    ? 'bg-emerald-50 text-emerald-700'
                    : 'bg-amber-50 text-amber-700'
                }`}
                title={quiz.verification.method}
              >
                {quiz.verification.status === 'verified' ? 'Verified' : 'Needs review'}
              </span>
            )}
          </div>
          <p className="mt-1 text-xs text-gray-500">
            Question {currentIndex + 1} of {quiz.questions.length} · Score {score.correct}/{score.total}
          </p>
        </div>
      </div>

      <div>
        <div className="mb-3 flex flex-wrap gap-2">
          {question.tags.map((tag) => (
            <span key={tag} className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-600">
              {tag}
            </span>
          ))}
        </div>
        <p className="text-base font-medium leading-7 text-gray-950">{question.stem}</p>
      </div>

      <div className="space-y-2">
        {question.options.map((option) => {
          const selected = selectedOptionId === option.id
          const answered = Boolean(answer)
          const isCorrect = question.correct_option_id === option.id
          return (
            <button
              key={option.id}
              type="button"
              disabled={answered || busy}
              onClick={() => setSelectedOptionId(option.id)}
              className={`flex w-full items-start gap-3 rounded-lg border p-3 text-left text-sm transition ${
                selected ? 'border-blue-300 bg-blue-50' : 'border-gray-200 bg-white hover:border-blue-200 hover:bg-blue-50/40'
              } ${answered && isCorrect ? 'border-emerald-300 bg-emerald-50' : ''}`}
            >
              <span className="mt-0.5 text-blue-700">
                {selected || (answered && isCorrect) ? <CheckCircle2 size={18} /> : <Circle size={18} />}
              </span>
              <span className="leading-6 text-gray-700">
                <span className="font-medium text-gray-950">{option.id}.</span> {option.text}
              </span>
            </button>
          )
        })}
      </div>

      {answer && (
        <div className={`rounded-lg p-3 text-sm ${answer.correct ? 'bg-emerald-50 text-emerald-900' : 'bg-red-50 text-red-900'}`}>
          <div className="font-medium">{answer.correct ? 'Correct' : 'Incorrect'}</div>
          <p className="mt-2 leading-6">{question.explanation}</p>
          {question.citations.length > 0 && (
            <QuizCitationReferences
              quizId={quiz.id}
              questionId={question.id}
              citations={question.citations}
              onSourceNavigate={onSourceNavigate}
            />
          )}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2 border-t border-gray-100 pt-3">
        <button
          className="inline-flex h-8 items-center rounded-lg border border-gray-200 px-3 text-xs font-medium text-gray-700 hover:bg-blue-50 disabled:opacity-50"
          type="button"
          disabled={currentIndex === 0}
          onClick={() => setCurrentIndex((value) => Math.max(0, value - 1))}
        >
          Previous
        </button>
        <button
          className="inline-flex h-8 items-center rounded-lg border border-gray-200 px-3 text-xs font-medium text-gray-700 hover:bg-blue-50 disabled:opacity-50"
          type="button"
          disabled={currentIndex >= quiz.questions.length - 1}
          onClick={() => setCurrentIndex((value) => Math.min(quiz.questions.length - 1, value + 1))}
        >
          Next
        </button>
        <button
          className="ml-auto inline-flex h-8 items-center rounded-lg bg-blue-600 px-3 text-xs font-medium text-white hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400"
          type="button"
          disabled={!selectedOptionId || Boolean(answer) || busy}
          onClick={submit}
        >
          Submit answer
        </button>
        <button
          className="inline-flex h-8 items-center rounded-lg border border-gray-200 px-3 text-xs font-medium text-gray-700 hover:bg-blue-50 disabled:opacity-50"
          type="button"
          disabled={busy || quiz.status === 'finished'}
          onClick={finish}
        >
          Finish quiz
        </button>
      </div>
    </div>
  )
}
function QuizCitationReferences({
  quizId,
  questionId,
  citations,
  onSourceNavigate,
}: {
  quizId: string
  questionId: string
  citations: QuizSession['questions'][number]['citations']
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const references = citations.map((citation, index) => quizCitationToSourceReference(citation, index))
  return (
    <SourceReferences
      id={`quiz-citations-${quizId}-${questionId}`}
      references={references}
      onNavigate={onSourceNavigate}
    />
  )
}

function quizCitationToSourceReference(
  citation: QuizSession['questions'][number]['citations'][number],
  index: number,
): SourceReference {
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

function quizCitationRawTarget(citation: QuizSession['questions'][number]['citations'][number]) {
  if (citation.kb && citation.document_id) {
    return ['kb', citation.kb, citation.document_id, citation.chunk_id].filter(Boolean).join(':')
  }
  return citation.source
}

function Composer({
  inputRef,
  input,
  setInput,
  capability,
  llmConfigs,
  activeLlmConfigId,
  knowledgeBases,
  selectedKnowledgeBaseId,
  selectedNotebookEnabled,
  onCapabilityChange,
  onKnowledgeBaseChange,
  onNotebookEnabledChange,
  onLlmConfigChange,
  onSend,
  onStop,
  attachments,
  onAddAttachments,
  onRemoveAttachment,
  mentions,
  onAddMention,
  onRemoveMention,
  disabled,
  running,
  variant,
}: {
  inputRef?: RefObject<HTMLTextAreaElement | null>
  input: string
  setInput: (value: string) => void
  capability: Capability
  llmConfigs: LlmModelConfig[]
  activeLlmConfigId: string | null
  knowledgeBases: Array<{ id: string; name: string }>
  selectedKnowledgeBaseId: string
  selectedNotebookEnabled: boolean
  onCapabilityChange: (capability: Capability) => void
  onKnowledgeBaseChange: (id: string) => void
  onNotebookEnabledChange: (enabled: boolean) => void
  onLlmConfigChange: (id: string) => void
  onSend: () => void
  onStop?: () => void
  attachments: ChatAttachment[]
  onAddAttachments: (attachments: ChatAttachment[]) => void
  onRemoveAttachment: (id: string) => void
  mentions: SpaceMention[]
  onAddMention: (mention: SpaceMention) => void
  onRemoveMention: (id: string) => void
  disabled: boolean
  running: boolean
  variant: 'center' | 'bottom'
}) {
  const { t } = useI18n()
  const [openMenu, setOpenMenu] = useState<OpenMenu>(null)
  const [readingAttachments, setReadingAttachments] = useState(false)
  const [spaceQuery, setSpaceQuery] = useState('')
  const [spaceMentionFilter] = useState<SpaceMentionFilter>('notebook_entry')
  const [spaceMentions, setSpaceMentions] = useState<SpaceMention[]>([])
  const [loadingSpaceMentions, setLoadingSpaceMentions] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const composerRef = useRef<HTMLDivElement>(null)
  const activeKnowledge = selectedNotebookEnabled
    ? { id: '__notebook__', name: 'Notebook' }
    : knowledgeBases.find((item) => item.id === selectedKnowledgeBaseId)
  const activeModel = llmConfigs.find((item) => item.id === activeLlmConfigId) ?? llmConfigs[0] ?? null
  const visibleSpaceMentions = useMemo(
    () => filterSpaceMentions(spaceMentions, spaceMentionFilter),
    [spaceMentions, spaceMentionFilter],
  )
  const knowledgeOptions = [
    {
      id: '',
      name: t('chat.knowledge.none'),
      description: t('chat.knowledge.none.description'),
      icon: <Database size={21} />,
    },
    ...knowledgeBases.map((item) => ({
      id: item.id,
      name: item.name,
      description: t('chat.knowledge.use.description'),
      icon: <Database size={21} />,
    })),
  ]
  const sourceOptions = [
    { ...knowledgeOptions[0]!, type: 'none' as const },
    {
      id: '__notebook__',
      type: 'notebook' as const,
      name: 'Notebook',
      description: t('chat.notebook.description'),
      icon: <FileText size={21} />,
    },
    ...knowledgeOptions.slice(1).map((item) => ({
      ...item,
      type: 'knowledge_base' as const,
    })),
  ]

  const toggleMenu = (menu: OpenMenu) => {
    if (disabled || running) return
    setOpenMenu((current) => (current === menu ? null : menu))
  }

  useEffect(() => {
    if (!openMenu) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!composerRef.current?.contains(event.target as Node)) setOpenMenu(null)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer, true)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer, true)
  }, [openMenu])

  const handleFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? [])
    event.target.value = ''
    if (files.length === 0) return

    setReadingAttachments(true)
    try {
      const parsed = await Promise.all(files.map(readChatAttachment))
      onAddAttachments(parsed)
    } finally {
      setReadingAttachments(false)
    }
  }

  useEffect(() => {
    if (openMenu !== 'space') return
    let cancelled = false
    const controller = new AbortController()
    setLoadingSpaceMentions(true)
    const timer = window.setTimeout(() => {
      const params = new URLSearchParams()
      if (spaceQuery.trim()) params.set('q', spaceQuery.trim())
      if (spaceMentionFilter !== 'all') params.set('type', spaceMentionFilter)
      params.set('limit', '50')
      fetch(`/api/space/mentions?${params.toString()}`, { signal: controller.signal })
        .then(async (res) => {
          const data = await res.json().catch(() => ({})) as { mentions?: SpaceMention[] }
          if (!res.ok) throw new Error(`HTTP ${res.status}`)
          if (!cancelled) setSpaceMentions(data.mentions ?? [])
        })
        .catch(() => {
          if (!cancelled) setSpaceMentions([])
        })
        .finally(() => {
          if (!cancelled) setLoadingSpaceMentions(false)
        })
    }, 160)

    return () => {
      cancelled = true
      controller.abort()
      window.clearTimeout(timer)
    }
  }, [openMenu, spaceQuery, spaceMentionFilter])

  return (
    <div
      ref={composerRef}
      className={`relative rounded-3xl border border-blue-100 bg-white shadow-sm ${
        variant === 'center' ? 'shadow-xl shadow-blue-950/5' : ''
      }`}
      onKeyDown={(event) => {
        if (event.key === 'Escape') setOpenMenu(null)
      }}
    >
      <textarea
        ref={inputRef}
        className={`${
          variant === 'center' ? 'min-h-36 text-base' : 'min-h-16 text-sm'
        } w-full resize-none rounded-t-3xl px-5 py-4 outline-none placeholder:text-gray-400 disabled:bg-white`}
        value={input}
        onChange={(event) => setInput(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault()
            onSend()
          }
        }}
        placeholder={t('chat.input.placeholder')}
      />
      {attachments.length > 0 && (
        <div className="border-t border-blue-50 px-4 py-2">
          <AttachmentSummary
            attachments={attachments}
            removable
            onRemove={onRemoveAttachment}
          />
        </div>
      )}
      {mentions.length > 0 && (
        <div className="border-t border-blue-50 px-4 py-2">
          <MentionSummary mentions={mentions} removable onRemove={onRemoveMention} />
        </div>
      )}
      <div className="relative flex flex-wrap items-center gap-2 border-t border-blue-50 px-4 py-2">
        <button
          className={`inline-flex h-9 items-center gap-2 rounded-full px-3 text-sm transition ${
            capability === 'research'
              ? 'bg-blue-100 text-blue-800'
              : 'text-gray-600 hover:bg-blue-50'
          } disabled:text-gray-400`}
          type="button"
          title={t('cap.research.description')}
          disabled={disabled || running}
          aria-pressed={capability === 'research'}
          onClick={() => onCapabilityChange(capability === 'research' ? 'chat' : 'research')}
        >
          <SearchCheck size={18} />
          {t('cap.research')}
        </button>

        <button
          className="inline-flex h-9 items-center gap-2 rounded-full px-3 text-sm text-gray-600 hover:bg-blue-50 disabled:text-gray-400"
          type="button"
          disabled={disabled || running || readingAttachments}
          onClick={() => fileInputRef.current?.click()}
        >
          <Paperclip size={18} />
          {t('chat.attachments')}
        </button>
        <input
          ref={fileInputRef}
          className="hidden"
          type="file"
          multiple
          onChange={handleFileChange}
        />

        <div className="relative">
          <ToolbarButton
            active={openMenu === 'knowledge'}
            icon={<Database size={18} />}
            label={activeKnowledge?.name ?? t('chat.knowledge.none')}
            onClick={() => toggleMenu('knowledge')}
          />
          {openMenu === 'knowledge' && (
            <DropdownPanel widthClassName="w-[17rem] max-w-[calc(100vw-1.5rem)]">
              {sourceOptions.map((item) => (
                <DropdownOption
                  key={item.id || 'none'}
                  selected={
                    item.type === 'notebook'
                      ? selectedNotebookEnabled
                      : item.type === 'none'
                        ? !selectedNotebookEnabled && !selectedKnowledgeBaseId
                        : !selectedNotebookEnabled && item.id === selectedKnowledgeBaseId
                  }
                  icon={item.icon}
                  title={item.name}
                  description={item.description}
                  onClick={() => {
                    if (item.type === 'notebook') {
                      onNotebookEnabledChange(true)
                    } else {
                      onKnowledgeBaseChange(item.id)
                    }
                    setOpenMenu(null)
                  }}
                />
              ))}
            </DropdownPanel>
          )}
        </div>

        <div className="relative">
          <ToolbarButton
            active={openMenu === 'space'}
            icon={<AtSign size={18} />}
            label={mentions.length > 0 ? `${t('nav.knowledge')} ${mentions.length}` : t('nav.knowledge')}
            onClick={() => toggleMenu('space')}
          />
          {openMenu === 'space' && (
            <DropdownPanel
              widthClassName="w-[18rem] max-w-[calc(100vw-1.5rem)]"
              className="flex max-h-[min(19rem,calc(100vh-7rem))] flex-col"
            >
              <div className="shrink-0 space-y-1.5 border-b border-blue-50 bg-white px-3 pb-1.5 pt-1">
                <input
                  className="h-6 w-full rounded-lg border border-blue-100 px-2 text-xs outline-none focus:border-blue-300"
                  value={spaceQuery}
                  onChange={(event) => setSpaceQuery(event.target.value)}
                  placeholder={t('chat.space.searchPlaceholder')}
                  autoFocus
                />
                {loadingSpaceMentions && (
                  <div className="px-1 text-[11px] text-blue-500">{t('chat.space.updating')}</div>
                )}
              </div>
              <div className="min-h-0 overflow-y-auto py-1">
                {visibleSpaceMentions.length === 0 ? (
                  <div className="px-3 py-2 text-xs text-gray-500">
                    {t('chat.space.noMatching')}
                  </div>
                ) : (
                  visibleSpaceMentions.map((mention) => (
                    <DropdownOption
                      key={mention.id}
                      selected={mentions.some((item) => item.id === mention.id)}
                      icon={spaceMentionIcon(mention)}
                      title={mention.title}
                      description={spaceMentionDescription(mention)}
                      onClick={() => {
                        onAddMention(mention)
                        setOpenMenu(null)
                      }}
                    />
                  ))
                )}
              </div>
            </DropdownPanel>
          )}
        </div>

        <div className="relative ml-auto">
          <ToolbarButton
            active={openMenu === 'model'}
            icon={<Brain size={16} />}
            label={activeModel?.model ?? t('chat.model.select')}
            onClick={() => toggleMenu('model')}
          />
          {openMenu === 'model' && (
            <DropdownPanel widthClassName="right-0 left-auto w-[18rem] max-w-[calc(100vw-1.5rem)]">
              {llmConfigs.length === 0 ? (
                <DropdownOption
                  selected
                  icon={<Brain size={21} />}
                  title={t('chat.model.none')}
                  description={t('chat.model.configureFirst')}
                  onClick={() => setOpenMenu(null)}
                />
              ) : (
                llmConfigs.map((config) => (
                  <DropdownOption
                    key={config.id}
                    selected={config.id === activeModel?.id}
                    icon={<Brain size={21} />}
                    title={config.name || config.model}
                    description={`${llmApiModeLabel(config.provider)} / ${config.model}`}
                    onClick={() => {
                      onLlmConfigChange(config.id)
                      setOpenMenu(null)
                    }}
                  />
                ))
              )}
            </DropdownPanel>
          )}
        </div>

        <button
          className={`flex h-9 w-9 items-center justify-center rounded-full text-white disabled:bg-gray-200 disabled:text-gray-400 ${
            running ? 'bg-gray-900 hover:bg-gray-800' : 'bg-blue-600 hover:bg-blue-700'
          }`}
          onClick={running ? onStop : onSend}
          disabled={disabled || (!running && !input.trim() && attachments.filter((attachment) => !attachment.error).length === 0 && mentions.length === 0)}
          type="button"
          title={running ? t('chat.stop') : t('chat.send')}
        >
          {running ? <Square size={15} /> : <ArrowUp size={20} />}
        </button>
      </div>
    </div>
  )
}

const MAX_ATTACHMENT_BYTES = 16 * 1024 * 1024
const MAX_ATTACHMENT_CHARS = 20000
const TEXT_EXTENSIONS = new Set([
  'txt',
  'md',
  'markdown',
  'csv',
  'tsv',
  'json',
  'jsonl',
  'log',
  'rs',
  'ts',
  'tsx',
  'js',
  'jsx',
  'py',
  'toml',
  'yaml',
  'yml',
  'xml',
  'html',
  'css',
  'sql',
])

async function readChatAttachment(file: File): Promise<ChatAttachment> {
  const base = {
    id: `${file.name}-${file.size}-${file.lastModified}-${Math.random().toString(36).slice(2)}`,
    name: file.name,
    size: file.size,
    type: file.type || 'application/octet-stream',
  }

  if (file.size > MAX_ATTACHMENT_BYTES) {
    return {
      ...base,
      error: `Attachment exceeds ${formatBytes(MAX_ATTACHMENT_BYTES)}. Split it before sending.`,
    }
  }

  if (!isTextFile(file) || isPdfFile(file)) {
    return parseAttachmentOnServer(file, base)
  }

  try {
    const raw = await file.text()
    const truncated = raw.length > MAX_ATTACHMENT_CHARS
    return {
      ...base,
      text: truncated ? raw.slice(0, MAX_ATTACHMENT_CHARS) : raw,
      truncated,
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ...base, error: `Read failed: ${message}` }
  }
}

async function parseAttachmentOnServer(
  file: File,
  base: Pick<ChatAttachment, 'id' | 'name' | 'size' | 'type'>,
): Promise<ChatAttachment> {
  const form = new FormData()
  form.append('file', file)
  try {
    const res = await fetch('/api/attachments/parse', {
      method: 'POST',
      body: form,
    })
    const data = await res.json().catch(() => ({})) as {
      attachment?: {
        name?: string
        size?: number
        mime_type?: string | null
        text?: string
        truncated?: boolean
      }
      error?: string
    }
    if (!res.ok || !data.attachment?.text) {
      return { ...base, error: data.error || `Attachment parse failed: HTTP ${res.status}` }
    }
    return {
      ...base,
      name: data.attachment.name || base.name,
      size: data.attachment.size ?? base.size,
      type: data.attachment.mime_type || base.type,
      text: data.attachment.text,
      truncated: Boolean(data.attachment.truncated),
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ...base, error: `Attachment parse request failed: ${message}` }
  }
}

function isPdfFile(file: File) {
  return file.type === 'application/pdf' || file.name.toLowerCase().endsWith('.pdf')
}

function isTextFile(file: File) {
  if (file.type.startsWith('text/')) return true
  const ext = file.name.split('.').pop()?.toLowerCase()
  return Boolean(ext && TEXT_EXTENSIONS.has(ext))
}

function AttachmentSummary({
  attachments,
  removable = false,
  onRemove,
}: {
  attachments: ChatAttachment[]
  removable?: boolean
  onRemove?: (id: string) => void
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {attachments.map((attachment) => (
        <div
          key={attachment.id}
          className={`flex max-w-full items-center gap-2 rounded-xl border px-3 py-2 text-xs ${
            attachment.error
              ? 'border-red-100 bg-red-50 text-red-700'
              : 'border-blue-100 bg-blue-50 text-gray-700'
          }`}
          title={attachment.error || attachment.name}
        >
          {attachment.error ? (
            <AlertCircle size={16} className="shrink-0" />
          ) : (
            <FileText size={16} className="shrink-0 text-blue-600" />
          )}
          <span className="min-w-0 truncate font-medium">{attachment.name}</span>
          <span className="shrink-0 text-gray-500">{formatBytes(attachment.size)}</span>
          {attachment.truncated && <span className="shrink-0 text-amber-600">truncated</span>}
          {attachment.error && <span className="min-w-0 truncate">{attachment.error}</span>}
          {removable && (
            <button
              className="ml-1 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-gray-500 hover:bg-white hover:text-gray-900"
              type="button"
              onClick={() => onRemove?.(attachment.id)}
              title="Remove attachment"
            >
              <X size={14} />
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

function MentionSummary({
  mentions,
  removable = false,
  onRemove,
}: {
  mentions: SpaceMention[]
  removable?: boolean
  onRemove?: (id: string) => void
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {mentions.map((mention) => (
        <div
          key={mention.id}
          className="flex max-w-full items-center gap-2 rounded-xl border border-blue-100 bg-white px-3 py-2 text-xs text-gray-700"
          title={mention.preview || mention.title}
        >
          <span className="shrink-0 text-blue-600">{spaceMentionIcon(mention, 16)}</span>
          <span className="shrink-0 rounded-full bg-blue-50 px-2 py-0.5 font-medium text-blue-700">
            {spaceMentionTypeLabel(mention)}
          </span>
          <span className="min-w-0 truncate font-medium">{mention.title}</span>
          {removable && (
            <button
              className="ml-1 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-gray-500 hover:bg-blue-50 hover:text-gray-900"
              type="button"
              onClick={() => onRemove?.(mention.id)}
              title="Remove Space reference"
            >
              <X size={14} />
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

function spaceMentionIcon(mention: SpaceMention, size = 21) {
  if (mention.type === 'notebook_entry') return <FileText size={size} />
  return <FileQuestion size={size} />
}

function spaceMentionTypeLabel(mention: SpaceMention) {
  if (mention.type === 'notebook_entry') return 'Note'
  if (mention.type === 'quiz_question') return 'Question'
  return 'Quiz'
}

function filterSpaceMentions(mentions: SpaceMention[], filter: SpaceMentionFilter) {
  if (filter === 'all') return mentions
  return mentions.filter((mention) => mention.type === filter)
}

function spaceMentionDescription(mention: SpaceMention) {
  return [spaceMentionTypeLabel(mention), mention.preview].filter(Boolean).join(' - ')
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function llmApiModeLabel(provider: LlmModelConfig['provider']) {
  if (provider === 'anthropic') return 'Anthropic Messages'
  return 'OpenAI-compatible'
}

function ToolbarButton({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean
  icon: ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <button
      className={`inline-flex h-9 max-w-56 items-center gap-2 rounded-full border px-3 text-sm transition ${
        active
          ? 'border-blue-200 bg-blue-50 text-blue-700 shadow-sm'
          : 'border-transparent text-gray-700 hover:bg-blue-50'
      }`}
      type="button"
      onClick={onClick}
    >
      <span className="shrink-0">{icon}</span>
      <span className="truncate">{label}</span>
      <ChevronDown size={14} className={`shrink-0 text-gray-400 transition-transform ${active ? 'rotate-180' : ''}`} />
    </button>
  )
}

function DropdownPanel({
  children,
  widthClassName,
  className = '',
}: {
  children: ReactNode
  widthClassName: string
  className?: string
}) {
  return (
    <div
      className={`absolute bottom-12 left-0 z-30 overflow-hidden rounded-lg border border-gray-200 bg-white shadow-xl shadow-blue-950/10 ${widthClassName} ${className || 'py-1'}`}
    >
      {children}
    </div>
  )
}

function DropdownOption({
  selected,
  icon,
  title,
  description,
  onClick,
}: {
  selected: boolean
  icon: ReactNode
  title: string
  description: string
  onClick: () => void
}) {
  return (
    <button
      className={`flex w-full items-center gap-2 px-2 py-2 text-left transition ${
        selected ? 'bg-blue-50' : 'hover:bg-gray-50'
      }`}
      type="button"
      onClick={onClick}
    >
      <span className={`flex h-7 w-7 shrink-0 items-center justify-center ${
        selected ? 'text-blue-700' : 'text-gray-500'
      }`}>{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-semibold text-gray-950">{title}</span>
        <span className="mt-0.5 block truncate text-xs text-gray-500">{description}</span>
      </span>
      {selected ? (
        <Check size={14} className="shrink-0 text-blue-600" />
      ) : (
        <span className="h-2 w-2 shrink-0 rounded-full bg-transparent" />
      )}
    </button>
  )
}

function MessageActionToolbar({
  align,
  copied,
  onCopy,
  onQuote,
  onEdit,
  onSaveToNotebook,
  onRegenerate,
  sourceCount = 0,
  onShowSources,
}: {
  align: 'left' | 'right'
  copied: boolean
  onCopy: () => void
  onQuote: () => void
  onEdit?: () => void
  onSaveToNotebook?: () => void
  onRegenerate?: () => void
  sourceCount?: number
  onShowSources?: () => void
}) {
  const buttonClassName = 'inline-flex h-7 min-w-7 items-center justify-center rounded-md px-1.5 text-gray-500 outline-none hover:bg-gray-100 hover:text-gray-900 focus-visible:bg-blue-50 focus-visible:text-blue-700'
  return (
    <div className="relative h-8 w-full" role="toolbar" aria-label="Message actions">
      <div
        className={`pointer-events-none absolute top-1 z-10 flex items-center gap-0.5 rounded-md border border-gray-200 bg-white p-0.5 opacity-0 shadow-sm transition-opacity group-hover/message:pointer-events-auto group-hover/message:opacity-100 group-focus-within/message:pointer-events-auto group-focus-within/message:opacity-100 ${
          align === 'right' ? 'right-0' : 'left-0'
        }`}
      >
        <button className={buttonClassName} type="button" title={copied ? 'Copied' : 'Copy'} aria-label={copied ? 'Copied' : 'Copy message'} onClick={onCopy}>
          {copied ? <Check size={15} /> : <Copy size={15} />}
        </button>
        <button className={buttonClassName} type="button" title="Quote" aria-label="Quote message" onClick={onQuote}>
          <Quote size={15} />
        </button>
        {onEdit && (
          <button className={buttonClassName} type="button" title="Edit and regenerate" aria-label="Edit and regenerate message" onClick={onEdit}>
            <Edit3 size={15} />
          </button>
        )}
        {onSaveToNotebook && (
          <button className={buttonClassName} type="button" title="Save to Notebook" aria-label="Save message to Notebook" onClick={onSaveToNotebook}>
            <FileText size={15} />
          </button>
        )}
        {onRegenerate && (
          <button className={buttonClassName} type="button" title="Regenerate" aria-label="Regenerate answer" onClick={onRegenerate}>
            <RefreshCw size={15} />
          </button>
        )}
        {onShowSources && sourceCount > 0 && (
          <button className={`${buttonClassName} gap-1 px-2 text-xs font-medium`} type="button" title="Show sources" onClick={onShowSources}>
            <BookOpen size={14} />
            Sources {sourceCount}
          </button>
        )}
      </div>
    </div>
  )
}

function isStructuredAssistantMessage(msg: Message, capability: Capability) {
  if (msg.role !== 'assistant') return false
  return Boolean(
    msg.quiz
    || msg.quizPlan
    || msg.researchPlan
    || (msg.deepSolve && msg.deepSolve.length > 0)
    || (capability === 'research' && (looksLikeResearchReport(msg.text) || msg.researchUnavailable)),
  )
}

function copyTextWithDocumentFallback(text: string) {
  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.setAttribute('readonly', '')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  textarea.remove()
}

function messageClassName(msg: Message, structuredAssistant = false) {
  if (msg.role === 'user') return 'group/message ml-auto flex w-full max-w-3xl flex-col items-end'
  if (msg.role === 'assistant') {
    return structuredAssistant
      ? 'w-full min-w-0 py-2'
      : 'group/message w-full min-w-0 py-2 text-gray-900'
  }

  const tones: Record<NonNullable<Message['kind']>, string> = {
    idle: 'bg-gray-50',
    thinking: 'bg-gray-50',
    tool: 'bg-amber-50',
    done: 'bg-gray-50',
    error: 'bg-red-50',
  }
  return `max-w-3xl rounded-lg p-3 ${tones[msg.kind ?? 'idle']}`
}

