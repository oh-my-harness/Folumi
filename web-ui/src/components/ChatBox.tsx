import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import type { ChangeEvent, ReactNode, RefObject } from 'react'
import {
  AlertCircle,
  ArrowUp,
  BookOpen,
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  Database,
  Edit3,
  FileText,
  Paperclip,
  Quote,
  RefreshCw,
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
import { SaveNotebookDialog, SaveNotebookOutcomeDialog } from './SaveNotebookDialog'

type OpenMenu = 'knowledge' | 'model' | null

export interface SaveToNotebookOptions {
  folderPath?: string
  newFolderPath?: string
  filePath?: string
  entryType?: 'chat_excerpt'
  title?: string
}

interface Message {
  role: 'user' | 'assistant' | 'status'
  text: string
  thinking?: string
  kind?: 'idle' | 'thinking' | 'tool' | 'done' | 'error'
  citations?: Citation[]
  deepSolve?: DeepSolveTraceEntry[]
  notebookEditProposal?: NotebookEditProposal
  attachments?: ChatAttachment[]
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
  streamingThinking: string
  contextStats: ContextStats
  llmConfigs: LlmModelConfig[]
  activeLlmConfigId: string | null
  knowledgeBases: Array<{ id: string; name: string }>
  selectedKnowledgeBaseIds: string[]
  notebookVaults: NotebookVaultInfo[]
  selectedNotebookVaultId: string | null
  initialDraft?: { id: number; text: string } | null
  onSend: (text: string, attachments?: ChatAttachment[]) => void
  onStop?: () => void
  onEditUserMessage?: (messageIndex: number, nextText: string) => void
  onAskDeepSolveStep?: (step: { id: string; title: string; summary?: string }) => void
  onKnowledgeBaseToggle: (id: string) => void
  onNotebookVaultToggle: (id: string) => void
  onLlmConfigChange: (id: string) => void
  notebookFolders?: string[]
  notebookEntryPaths?: string[]
  notebookVault?: NotebookVaultInfo | null
  onSaveToNotebook?: (markdown: string, options?: SaveToNotebookOptions) => Promise<SaveToNotebookResult>
  onOpenNotebookEntry?: (entryId: string) => void
  onApplyNotebookEdit?: (proposal: NotebookEditProposal) => Promise<void>
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
  streamingThinking,
  contextStats,
  llmConfigs,
  activeLlmConfigId,
  knowledgeBases,
  selectedKnowledgeBaseIds,
  notebookVaults,
  selectedNotebookVaultId,
  initialDraft,
  onSend,
  onStop,
  onEditUserMessage,
  onAskDeepSolveStep,
  onKnowledgeBaseToggle,
  onNotebookVaultToggle,
  onLlmConfigChange,
  notebookFolders = [],
  notebookEntryPaths = [],
  notebookVault,
  onSaveToNotebook,
  onOpenNotebookEntry,
  onApplyNotebookEdit,
  onSourceNavigate,
  disabled,
  running = false,
}: Props) {
  const { t } = useI18n()
  const [input, setInput] = useState('')
  const [editingMessageIndex, setEditingMessageIndex] = useState<number | null>(null)
  const [editingMessageText, setEditingMessageText] = useState('')
  const [attachments, setAttachments] = useState<ChatAttachment[]>([])
  const [saveNotebookMarkdown, setSaveNotebookMarkdown] = useState<string | null>(null)
  const [saveNotebookFolder, setSaveNotebookFolder] = useState('')
  const [saveNotebookNewFolder, setSaveNotebookNewFolder] = useState('')
  const [saveNotebookFileName, setSaveNotebookFileName] = useState('')
  const [saveNotebookBusy, setSaveNotebookBusy] = useState(false)
  const [saveNotebookNative, setSaveNotebookNative] = useState(false)
  const [saveNotebookEntryType, setSaveNotebookEntryType] = useState<'chat_excerpt'>('chat_excerpt')
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
  const empty = messages.length === 0 && !streamingText && !streamingThinking

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
    if ((!input.trim() && readyAttachments.length === 0) || disabled || running) return
    shouldStickToBottomRef.current = true
    onSend(input.trim(), readyAttachments)
    setInput('')
    setAttachments([])
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

  const openSaveNotebookDialog = async (
    markdown: string,
    entryType: 'chat_excerpt' = 'chat_excerpt',
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
    entryType: 'chat_excerpt',
    title: string,
  ) => {
    if (!onSaveToNotebook || !notebookVault) return
    setSaveNotebookMarkdown(markdown)
    try {
      const selectedPath = await chooseDesktopSavePath(
        '保存到笔记',
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
        throw new Error('该位置已经存在同名笔记，请选择其他文件名。')
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
    if (messages.length === 0 && !streamingText && !streamingThinking) return

    el.scrollTop = restoredScrollTop(pending.position, el.scrollHeight, el.clientHeight)
    shouldStickToBottomRef.current = pending.position?.atBottom ?? true
    pendingScrollRestoreRef.current = null
  }, [messages.length, sessionId, streamingText, streamingThinking])

  useEffect(() => {
    const el = scrollRef.current
    if (!el || !shouldStickToBottomRef.current) return

    el.scrollTop = el.scrollHeight
  }, [messages, streamingText, streamingThinking])

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
              <p className="mt-2 text-sm text-gray-500">{t('chat.empty.description')}</p>
            </div>
            <Composer
              inputRef={composerInputRef}
              input={input}
              setInput={setInput}
              llmConfigs={llmConfigs}
              activeLlmConfigId={activeLlmConfigId}
              knowledgeBases={knowledgeBases}
              selectedKnowledgeBaseIds={selectedKnowledgeBaseIds}
              notebookVaults={notebookVaults}
              selectedNotebookVaultId={selectedNotebookVaultId}
              onKnowledgeBaseToggle={onKnowledgeBaseToggle}
              onNotebookVaultToggle={onNotebookVaultToggle}
              onLlmConfigChange={onLlmConfigChange}
              onSend={handleSend}
              onStop={onStop}
              attachments={attachments}
              onAddAttachments={handleAddAttachments}
              onRemoveAttachment={handleRemoveAttachment}
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
              const structuredAssistant = isStructuredAssistantMessage(msg)
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
                  msg.deepSolve && msg.deepSolve.length > 0 ? (
                    <DeepSolveMessage
                      text={msg.text}
                      events={msg.deepSolve}
                      citations={msg.citations}
                      citationList={(citations) => <CitationList citations={citations} onSourceNavigate={onSourceNavigate} />}
                      onAskStep={onAskDeepSolveStep}
                    />
                  ) : (
                    <>
                      <div className="assistant-message-surface">
                        {msg.thinking && <ThinkingDisclosure text={msg.thinking} />}
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
                          msg.text.trim() && onSaveToNotebook
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
            {(streamingText || streamingThinking) && (
              <div className="w-full min-w-0 py-2 text-gray-900" aria-live="polite">
                <div className="assistant-message-surface">
                  {streamingThinking && <ThinkingDisclosure text={streamingThinking} active />}
                  {streamingText && <MarkdownMessage text={streamingText} onSourceNavigate={onSourceNavigate} />}
                  {streamingText && <span className="inline-block h-4 w-0.5 animate-pulse bg-gray-700 align-text-bottom" />}
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
              llmConfigs={llmConfigs}
              activeLlmConfigId={activeLlmConfigId}
              knowledgeBases={knowledgeBases}
              selectedKnowledgeBaseIds={selectedKnowledgeBaseIds}
              notebookVaults={notebookVaults}
              selectedNotebookVaultId={selectedNotebookVaultId}
              onKnowledgeBaseToggle={onKnowledgeBaseToggle}
              onNotebookVaultToggle={onNotebookVaultToggle}
              onLlmConfigChange={onLlmConfigChange}
              onSend={handleSend}
              onStop={onStop}
              attachments={attachments}
              onAddAttachments={handleAddAttachments}
              onRemoveAttachment={handleRemoveAttachment}
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

function ThinkingDisclosure({ text, active = false }: { text: string; active?: boolean }) {
  const [expanded, setExpanded] = useState(active)
  const visible = active || expanded
  return (
    <div className="mb-2 max-w-3xl text-xs leading-5 text-gray-400">
      <button
        type="button"
        className="inline-flex items-center gap-1 rounded px-1 py-0.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700"
        onClick={() => setExpanded((value) => !value)}
        aria-expanded={visible}
      >
        {visible ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        <span>{active ? '思考中' : '思考过程'}</span>
      </button>
      {visible && (
        <div className={`mt-1 whitespace-pre-wrap border-l border-gray-200 pl-3 ${active ? 'max-h-20 overflow-hidden [mask-image:linear-gradient(to_bottom,black_65%,transparent)]' : ''}`}>
          {text}
        </div>
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

function citationRawTarget(citation: Citation) {
  if (citation.kb && citation.documentId) {
    return ['kb', citation.kb, citation.documentId, citation.chunkId].filter(Boolean).join(':')
  }
  return citation.rawSource || citation.source
}

function Composer({
  inputRef,
  input,
  setInput,
  llmConfigs,
  activeLlmConfigId,
  knowledgeBases,
  selectedKnowledgeBaseIds,
  notebookVaults,
  selectedNotebookVaultId,
  onKnowledgeBaseToggle,
  onNotebookVaultToggle,
  onLlmConfigChange,
  onSend,
  onStop,
  attachments,
  onAddAttachments,
  onRemoveAttachment,
  disabled,
  running,
  variant,
}: {
  inputRef?: RefObject<HTMLTextAreaElement | null>
  input: string
  setInput: (value: string) => void
  llmConfigs: LlmModelConfig[]
  activeLlmConfigId: string | null
  knowledgeBases: Array<{ id: string; name: string }>
  selectedKnowledgeBaseIds: string[]
  notebookVaults: NotebookVaultInfo[]
  selectedNotebookVaultId: string | null
  onKnowledgeBaseToggle: (id: string) => void
  onNotebookVaultToggle: (id: string) => void
  onLlmConfigChange: (id: string) => void
  onSend: () => void
  onStop?: () => void
  attachments: ChatAttachment[]
  onAddAttachments: (attachments: ChatAttachment[]) => void
  onRemoveAttachment: (id: string) => void
  disabled: boolean
  running: boolean
  variant: 'center' | 'bottom'
}) {
  const { t } = useI18n()
  const [openMenu, setOpenMenu] = useState<OpenMenu>(null)
  const [readingAttachments, setReadingAttachments] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const composerRef = useRef<HTMLDivElement>(null)
  const activeModel = llmConfigs.find((item) => item.id === activeLlmConfigId) ?? llmConfigs[0] ?? null
  const selectedSourceCount = selectedKnowledgeBaseIds.length + (selectedNotebookVaultId ? 1 : 0)
  const sourceOptions = [
    ...notebookVaults.map((vault) => ({
      id: vault.id,
      type: 'notebook' as const,
      name: `${t('nav.notebook')} · ${vault.name}`,
      description: t('chat.notebook.description'),
      icon: <FileText size={21} />,
      disabled: !vault.available,
    })),
    ...knowledgeBases.map((item) => ({
      id: item.id,
      type: 'knowledge_base' as const,
      name: item.name,
      description: t('chat.knowledge.use.description'),
      icon: <Database size={21} />,
      disabled: false,
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
      <div className="relative flex flex-wrap items-center gap-2 border-t border-blue-50 px-4 py-2">

        <button
          className="inline-flex h-9 items-center gap-2 rounded-xl border border-gray-200 bg-gray-50/80 px-3 text-sm text-gray-600 transition hover:border-blue-200 hover:bg-blue-50 disabled:border-gray-100 disabled:bg-gray-50 disabled:text-gray-400"
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
            label={selectedSourceCount > 0 ? `${t('chat.source.selected')} ${selectedSourceCount}` : t('chat.source.select')}
            onClick={() => toggleMenu('knowledge')}
          />
          {openMenu === 'knowledge' && (
            <DropdownPanel
              widthClassName="w-[21rem] max-w-[calc(100vw-1.5rem)]"
              title={t('chat.source.menu.title')}
              description={t('chat.source.menu.description')}
            >
              <div className="space-y-1 p-2">
                {sourceOptions.map((item) => (
                  <DropdownOption
                    key={item.id}
                    selected={
                      item.type === 'notebook'
                        ? selectedNotebookVaultId === item.id
                        : selectedKnowledgeBaseIds.includes(item.id)
                    }
                    icon={item.icon}
                    title={item.name}
                    description={item.description}
                    disabled={item.disabled}
                    onClick={() => {
                      if (item.type === 'notebook') {
                        onNotebookVaultToggle(item.id)
                      } else {
                        onKnowledgeBaseToggle(item.id)
                      }
                    }}
                  />
                ))}
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
            <DropdownPanel
              widthClassName="right-0 left-auto w-[21rem] max-w-[calc(100vw-1.5rem)]"
              title={t('chat.model.menu.title')}
              description={t('chat.model.menu.description')}
            >
              <div className="space-y-1 p-2">
                {llmConfigs.length === 0 ? (
                  <DropdownOption
                    selected={false}
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
              </div>
            </DropdownPanel>
          )}
        </div>

        <button
          className={`flex h-9 w-9 items-center justify-center rounded-full text-white disabled:bg-gray-200 disabled:text-gray-400 ${
            running ? 'bg-gray-900 hover:bg-gray-800' : 'bg-blue-600 hover:bg-blue-700'
          }`}
          onClick={running ? onStop : onSend}
          disabled={disabled || (!running && !input.trim() && attachments.filter((attachment) => !attachment.error).length === 0)}
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
      className={`inline-flex h-9 max-w-56 items-center gap-2 rounded-xl border px-3 text-sm transition ${
        active
          ? 'border-blue-300 bg-blue-50 text-blue-700 shadow-sm ring-2 ring-blue-100/70'
          : 'border-gray-200 bg-gray-50/80 text-gray-700 hover:border-blue-200 hover:bg-blue-50'
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
  title,
  description,
}: {
  children: ReactNode
  widthClassName: string
  className?: string
  title: string
  description: string
}) {
  return (
    <div
      className={`absolute bottom-[calc(100%+0.75rem)] left-0 z-40 overflow-hidden rounded-2xl border border-gray-200/90 bg-white shadow-[0_20px_55px_-20px_rgba(15,23,42,0.35)] ring-1 ring-gray-950/5 ${widthClassName} ${className}`}
      data-composer-dropdown="true"
    >
      <div className="shrink-0 border-b border-gray-100 bg-gray-50/80 px-4 py-3">
        <div className="text-sm font-semibold text-gray-950">{title}</div>
        <div className="mt-0.5 text-xs leading-5 text-gray-500">{description}</div>
      </div>
      {children}
    </div>
  )
}

function DropdownOption({
  selected,
  icon,
  title,
  description,
  disabled = false,
  onClick,
}: {
  selected: boolean
  icon: ReactNode
  title: string
  description: string
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <button
      className={`group flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition disabled:cursor-not-allowed disabled:opacity-50 ${
        selected
          ? 'border-blue-200 bg-blue-50/80 shadow-sm'
          : 'border-transparent hover:border-gray-200 hover:bg-gray-50'
      }`}
      type="button"
      disabled={disabled}
      onClick={onClick}
    >
      <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border ${
        selected
          ? 'border-blue-200 bg-white text-blue-700'
          : 'border-gray-200 bg-white text-gray-500 group-hover:text-gray-700'
      }`}>{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-gray-950">{title}</span>
        <span className="mt-0.5 block line-clamp-2 text-xs leading-4 text-gray-500">{description}</span>
      </span>
      {selected ? (
        <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-blue-600 text-white">
          <Check size={12} />
        </span>
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

function isStructuredAssistantMessage(msg: Message) {
  return msg.role === 'assistant' && Boolean(msg.deepSolve && msg.deepSolve.length > 0)
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
