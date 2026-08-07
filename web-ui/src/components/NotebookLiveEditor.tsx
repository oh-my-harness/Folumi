import Vditor from 'vditor'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { KeyboardEvent, MouseEvent } from 'react'
import 'vditor/dist/index.css'
import { joinNotebookEditorDocument, splitNotebookEditorDocument } from '../notebookEditorDocument'
import { notebookWikiTargetAtOffset } from '../notebookWikiLinks'

type SaveState = 'saved' | 'dirty' | 'saving' | 'error'

interface Props {
  entryId: string
  markdown: string
  language: 'zh-CN' | 'en-US'
  onSave: (entryId: string, markdown: string) => Promise<boolean>
  onWikiLinkOpen: (target: string) => void
}

export function NotebookLiveEditor({ entryId, markdown, language, onSave, onWikiLinkOpen }: Props) {
  const english = language === 'en-US'
  const rootRef = useRef<HTMLDivElement | null>(null)
  const editorRef = useRef<Vditor | null>(null)
  const onSaveRef = useRef(onSave)
  const frontMatterRef = useRef(splitNotebookEditorDocument(markdown).frontMatter)
  const latestDraftRef = useRef(markdown)
  const savedDraftRef = useRef(markdown)
  const saveQueueRef = useRef(Promise.resolve())
  const saveTimerRef = useRef<number | null>(null)
  const applyingExternalValueRef = useRef(false)
  const mountedRef = useRef(true)
  const [saveState, setSaveState] = useState<SaveState>('saved')

  onSaveRef.current = onSave

  const clearSaveTimer = useCallback(() => {
    if (saveTimerRef.current === null) return
    window.clearTimeout(saveTimerRef.current)
    saveTimerRef.current = null
  }, [])

  const enqueueSave = useCallback((showState = true) => {
    clearSaveTimer()
    saveQueueRef.current = saveQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        const next = latestDraftRef.current
        if (next === savedDraftRef.current) {
          if (showState && mountedRef.current) setSaveState('saved')
          return
        }
        if (showState && mountedRef.current) setSaveState('saving')
        const saved = await onSaveRef.current(entryId, next)
        if (!saved) {
          if (showState && mountedRef.current) setSaveState('error')
          return
        }
        savedDraftRef.current = next
        if (showState && mountedRef.current) {
          setSaveState(latestDraftRef.current === next ? 'saved' : 'dirty')
        }
      })
    return saveQueueRef.current
  }, [clearSaveTimer, entryId])

  const scheduleSave = useCallback(() => {
    clearSaveTimer()
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null
      void enqueueSave()
    }, 800)
  }, [clearSaveTimer, enqueueSave])

  const acceptEditorValue = useCallback((body: string) => {
    if (applyingExternalValueRef.current) return
    const nextMarkdown = joinNotebookEditorDocument(frontMatterRef.current, body)
    latestDraftRef.current = nextMarkdown
    if (nextMarkdown === savedDraftRef.current) {
      setSaveState('saved')
      clearSaveTimer()
      return
    }
    setSaveState('dirty')
    scheduleSave()
  }, [clearSaveTimer, scheduleSave])

  useLayoutEffect(() => {
    mountedRef.current = true
    const root = rootRef.current
    if (!root) return

    let disposed = false
    let ready = false
    const host = document.createElement('div')
    root.replaceChildren(host)
    const initialDocument = splitNotebookEditorDocument(latestDraftRef.current)
    frontMatterRef.current = initialDocument.frontMatter
    let editor: Vditor | null = new Vditor(host, {
      mode: 'ir',
      value: initialDocument.body,
      lang: english ? 'en_US' : 'zh_CN',
      theme: document.documentElement.classList.contains('dark') ? 'dark' : 'classic',
      cdn: new URL('vditor', document.baseURI).href.replace(/\/$/, ''),
      cache: { enable: false },
      toolbar: [],
      counter: { enable: false },
      resize: { enable: false },
      outline: { enable: false, position: 'left' },
      height: '100%',
      minHeight: 420,
      placeholder: english ? 'Start writing…' : '开始写笔记…',
      link: { isOpen: false },
      after: () => {
        if (!editor) return
        if (disposed) {
          editor.destroy()
          editor = null
          host.remove()
          return
        }
        editorRef.current = editor
        const currentDocument = splitNotebookEditorDocument(latestDraftRef.current)
        frontMatterRef.current = currentDocument.frontMatter
        if (currentDocument.body !== initialDocument.body) {
          applyingExternalValueRef.current = true
          editor.setValue(currentDocument.body, true)
          applyingExternalValueRef.current = false
        }
        ready = true
      },
      input: (value) => {
        if (!disposed && ready) acceptEditorValue(value)
      },
      blur: (value) => {
        if (disposed || !ready) return
        root.querySelectorAll('.vditor-ir__node--expand').forEach((node) => {
          node.classList.remove('vditor-ir__node--expand')
        })
        acceptEditorValue(value)
        void enqueueSave()
      },
    })

    return () => {
      disposed = true
      mountedRef.current = false
      clearSaveTimer()
      if (latestDraftRef.current !== savedDraftRef.current) void enqueueSave(false)
      if (editor && editorRef.current === editor) {
        editorRef.current = null
        editor.destroy()
        editor = null
      }
      host.remove()
    }
  }, [acceptEditorValue, clearSaveTimer, english, enqueueSave])

  useEffect(() => {
    const editor = editorRef.current
    if (!editor || markdown === latestDraftRef.current) return
    if (latestDraftRef.current !== savedDraftRef.current) return

    const externalDocument = splitNotebookEditorDocument(markdown)
    applyingExternalValueRef.current = true
    frontMatterRef.current = externalDocument.frontMatter
    latestDraftRef.current = markdown
    savedDraftRef.current = markdown
    editor.setValue(externalDocument.body, true)
    applyingExternalValueRef.current = false
    setSaveState('saved')
  }, [markdown])

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLocaleLowerCase() !== 's') return
    event.preventDefault()
    const current = editorRef.current?.getValue()
    if (current !== undefined) {
      latestDraftRef.current = joinNotebookEditorDocument(frontMatterRef.current, current)
    }
    void enqueueSave()
  }, [enqueueSave])

  const handleClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    if (!event.ctrlKey && !event.metaKey) return
    const caret = caretTextOffsetAtPoint(event.currentTarget, event.clientX, event.clientY)
    if (!caret) return
    const wikiTarget = notebookWikiTargetAtOffset(caret.text, caret.offset)
    if (!wikiTarget) return
    event.preventDefault()
    event.stopPropagation()
    onWikiLinkOpen(wikiTarget)
  }, [onWikiLinkOpen])

  const statusLabel = saveState === 'saving'
    ? (english ? 'Saving…' : '正在保存…')
    : saveState === 'dirty'
      ? (english ? 'Unsaved' : '尚未保存')
      : saveState === 'error'
        ? (english ? 'Save failed' : '保存失败')
        : (english ? 'Saved' : '已保存')

  return (
    <div
      className="notebook-direct-editor mx-auto flex min-h-full w-full max-w-4xl flex-col px-5"
      data-notebook-direct-editor="true"
      onClickCapture={handleClick}
      onKeyDown={handleKeyDown}
    >
      <div ref={rootRef} className="notebook-ir-editor min-h-[calc(100vh-205px)] flex-1" />
      <div
        className={`pointer-events-none sticky bottom-0 h-8 bg-gradient-to-t from-white via-white/90 to-transparent pt-2 text-right text-[11px] ${saveState === 'error' ? 'text-red-600' : 'text-gray-400'}`}
        aria-live="polite"
      >
        {statusLabel}
      </div>
    </div>
  )
}

function caretTextOffsetAtPoint(root: HTMLElement, x: number, y: number) {
  const documentWithCaret = root.ownerDocument as Document & {
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null
    caretRangeFromPoint?: (x: number, y: number) => Range | null
  }
  const position = documentWithCaret.caretPositionFromPoint?.(x, y)
  const range = position ? undefined : documentWithCaret.caretRangeFromPoint?.(x, y)
  const node = position?.offsetNode ?? range?.startContainer
  const nodeOffset = position?.offset ?? range?.startOffset
  if (!node || nodeOffset === undefined) return undefined

  const element = node instanceof Element ? node : node.parentElement
  const block = element?.closest<HTMLElement>('[data-block="0"], [data-type="NodeParagraph"], [data-type="NodeHeading"]')
  if (!block || !root.contains(block)) return undefined

  let offset = 0
  const walker = root.ownerDocument.createTreeWalker(block, NodeFilter.SHOW_TEXT)
  for (let current = walker.nextNode(); current; current = walker.nextNode()) {
    if (current === node) return { text: block.textContent ?? '', offset: offset + nodeOffset }
    offset += current.textContent?.length ?? 0
  }
  return undefined
}
