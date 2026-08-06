import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent, MouseEvent, ReactNode } from 'react'
import { replaceNotebookMarkdownBlock, splitNotebookMarkdownBlocks } from '../notebookLivePreview'

type SaveState = 'saved' | 'dirty' | 'saving' | 'error'

interface ActiveBlock {
  start: number
  end: number
  source: string
}

interface Props {
  entryId: string
  markdown: string
  language: 'zh-CN' | 'en-US'
  renderMarkdown: (markdown: string) => ReactNode
  onSave: (entryId: string, markdown: string) => Promise<boolean>
}

export function NotebookLiveEditor({ entryId, markdown, language, renderMarkdown, onSave }: Props) {
  const english = language === 'en-US'
  const [draft, setDraft] = useState(markdown)
  const [activeBlock, setActiveBlock] = useState<ActiveBlock | null>(null)
  const [saveState, setSaveState] = useState<SaveState>('saved')
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const latestDraftRef = useRef(markdown)
  const savedDraftRef = useRef(markdown)
  const saveQueueRef = useRef(Promise.resolve())
  const blocks = useMemo(() => splitNotebookMarkdownBlocks(draft), [draft])

  const enqueueSave = useCallback(() => {
    saveQueueRef.current = saveQueueRef.current.then(async () => {
      const next = latestDraftRef.current
      if (!next.trim() || next === savedDraftRef.current) {
        setSaveState('saved')
        return
      }
      setSaveState('saving')
      const saved = await onSave(entryId, next)
      if (saved) {
        savedDraftRef.current = next
        setSaveState(latestDraftRef.current === next ? 'saved' : 'dirty')
      } else {
        setSaveState('error')
      }
    })
    return saveQueueRef.current
  }, [entryId, onSave])

  useEffect(() => {
    setDraft(markdown)
    latestDraftRef.current = markdown
    savedDraftRef.current = markdown
    setActiveBlock(null)
    setSaveState('saved')
  }, [entryId])

  useEffect(() => {
    if (latestDraftRef.current !== savedDraftRef.current || activeBlock) return
    setDraft(markdown)
    latestDraftRef.current = markdown
    savedDraftRef.current = markdown
  }, [activeBlock, markdown])

  useEffect(() => {
    latestDraftRef.current = draft
    if (draft === savedDraftRef.current || !draft.trim()) return
    setSaveState('dirty')
    const timer = window.setTimeout(() => { void enqueueSave() }, 800)
    return () => window.clearTimeout(timer)
  }, [draft, enqueueSave])

  useEffect(() => () => {
    if (latestDraftRef.current.trim() && latestDraftRef.current !== savedDraftRef.current) {
      void onSave(entryId, latestDraftRef.current)
    }
  }, [entryId, onSave])

  useLayoutEffect(() => {
    const textarea = textareaRef.current
    if (!textarea) return
    textarea.style.height = '0px'
    textarea.style.height = `${Math.max(42, textarea.scrollHeight)}px`
  }, [activeBlock?.source])

  const startEditing = useCallback((event: MouseEvent, start: number, end: number, source: string) => {
    const target = event.target
    if (target instanceof Element && target.closest('a, button, input')) return
    setActiveBlock({ start, end, source })
  }, [])

  const updateActiveBlock = useCallback((source: string) => {
    if (!activeBlock) return
    setDraft((value) => {
      const next = replaceNotebookMarkdownBlock(value, activeBlock.start, activeBlock.end, source)
      latestDraftRef.current = next
      return next
    })
    setActiveBlock({ start: activeBlock.start, end: activeBlock.start + source.length, source })
  }, [activeBlock])

  const finishEditing = useCallback(() => {
    setActiveBlock(null)
    void enqueueSave()
  }, [enqueueSave])

  const handleEditorKeyDown = useCallback((event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase() === 's') {
      event.preventDefault()
      void enqueueSave()
      return
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      event.currentTarget.blur()
      return
    }
    if (event.key === 'Tab') {
      event.preventDefault()
      const textarea = event.currentTarget
      const start = textarea.selectionStart
      const end = textarea.selectionEnd
      const next = `${textarea.value.slice(0, start)}  ${textarea.value.slice(end)}`
      updateActiveBlock(next)
      window.requestAnimationFrame(() => {
        textarea.selectionStart = start + 2
        textarea.selectionEnd = start + 2
      })
    }
  }, [enqueueSave, updateActiveBlock])

  const before = activeBlock ? blocks.filter((block) => block.end + block.separator.length <= activeBlock.start) : blocks
  const after = activeBlock ? blocks.filter((block) => block.start >= activeBlock.end) : []
  const statusLabel = saveState === 'saving'
    ? (english ? 'Saving…' : '正在保存…')
    : saveState === 'dirty'
      ? (english ? 'Unsaved changes' : '尚未保存')
      : saveState === 'error'
        ? (english ? 'Save failed' : '保存失败')
        : (english ? 'Saved' : '已保存')

  const renderBlock = (block: (typeof blocks)[number]) => (
    <div
      key={`${block.start}:${block.end}`}
      className="group/live-block min-h-7 cursor-text rounded-md px-2 py-1 transition-colors hover:bg-blue-50/40"
      style={{ marginBottom: block.separator ? '0.65rem' : undefined }}
      data-notebook-live-block="true"
      onClick={(event) => startEditing(event, block.start, block.end, block.source)}
    >
      {block.source ? renderMarkdown(block.source) : <span className="block h-5" />}
    </div>
  )

  return (
    <div className="mx-auto flex min-h-full max-w-4xl flex-col px-5 py-5" data-notebook-live-preview="true">
      <div className="min-h-[calc(100vh-220px)] rounded-xl border border-gray-200 bg-gray-50/60 px-4 py-4">
        {before.map(renderBlock)}
        {activeBlock && (
          <textarea
            ref={textareaRef}
            autoFocus
            className="block w-full resize-none overflow-hidden rounded-lg border border-blue-300 bg-white px-3 py-2 font-mono text-sm leading-6 text-gray-900 outline-none ring-2 ring-blue-100"
            value={activeBlock.source}
            aria-label={english ? 'Edit current Markdown block' : '编辑当前 Markdown 块'}
            spellCheck={false}
            onFocus={(event) => {
              const length = event.currentTarget.value.length
              event.currentTarget.setSelectionRange(length, length)
            }}
            onChange={(event) => updateActiveBlock(event.target.value)}
            onBlur={finishEditing}
            onKeyDown={handleEditorKeyDown}
          />
        )}
        {after.map(renderBlock)}
        {!activeBlock && (
          <button
            type="button"
            className="mt-2 h-12 w-full cursor-text rounded-md text-left text-sm text-gray-300 hover:bg-blue-50/40 hover:text-gray-400"
            onClick={() => setActiveBlock({ start: draft.length, end: draft.length, source: '' })}
          >
            {draft.trim() ? '' : (english ? 'Click to start writing…' : '点击开始输入…')}
          </button>
        )}
      </div>
      <div className={`mt-2 h-5 text-right text-xs ${saveState === 'error' ? 'text-red-600' : 'text-gray-400'}`} aria-live="polite">
        {statusLabel}
      </div>
    </div>
  )
}
