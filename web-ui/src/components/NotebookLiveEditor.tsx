import { CrepeBuilder } from '@milkdown/crepe/builder'
import { codeMirror } from '@milkdown/crepe/feature/code-mirror'
import { latex } from '@milkdown/crepe/feature/latex'
import { linkTooltip } from '@milkdown/crepe/feature/link-tooltip'
import { listItem } from '@milkdown/crepe/feature/list-item'
import { placeholder } from '@milkdown/crepe/feature/placeholder'
import { table } from '@milkdown/crepe/feature/table'
import { replaceAll } from '@milkdown/kit/utils'
import { useCallback, useEffect, useRef, useState } from 'react'
import type { KeyboardEvent, MouseEvent } from 'react'
import '@milkdown/crepe/theme/common/style.css'
import '@milkdown/crepe/theme/frame.css'
import { joinNotebookEditorDocument, splitNotebookEditorDocument } from '../notebookEditorDocument'
import { notebookEditorLanguages } from '../notebookEditorLanguages'
import {
  notebookWikiTargetFromHref,
  prepareNotebookMarkdownForEditor,
  restoreNotebookMarkdownFromEditor,
} from '../notebookWikiLinks'

type SaveState = 'saved' | 'dirty' | 'saving' | 'error'

interface Props {
  entryId: string
  markdown: string
  language: 'zh-CN' | 'en-US'
  onSave: (entryId: string, markdown: string) => Promise<boolean>
  onWikiLinkOpen: (target: string) => void
}

export function NotebookLiveEditor({
  entryId,
  markdown,
  language,
  onSave,
  onWikiLinkOpen,
}: Props) {
  const english = language === 'en-US'
  const rootRef = useRef<HTMLDivElement | null>(null)
  const crepeRef = useRef<CrepeBuilder | null>(null)
  const onSaveRef = useRef(onSave)
  const frontMatterRef = useRef(splitNotebookEditorDocument(markdown).frontMatter)
  const latestDraftRef = useRef(markdown)
  const savedDraftRef = useRef(markdown)
  const saveQueueRef = useRef(Promise.resolve())
  const saveTimerRef = useRef<number | null>(null)
  const applyingExternalValueRef = useRef(false)
  const userEditObservedRef = useRef(false)
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

  useEffect(() => {
    mountedRef.current = true
    const root = rootRef.current
    if (!root) return

    let disposed = false
    const initialDocument = splitNotebookEditorDocument(latestDraftRef.current)
    frontMatterRef.current = initialDocument.frontMatter
    const crepe = new CrepeBuilder({
      root,
      defaultValue: prepareNotebookMarkdownForEditor(initialDocument.body),
    })
      .addFeature(listItem)
      .addFeature(linkTooltip)
      .addFeature(placeholder, {
        mode: 'doc',
        text: english ? 'Start writing…' : '开始写笔记…',
      })
      .addFeature(codeMirror, { languages: notebookEditorLanguages })
      .addFeature(table)
      .addFeature(latex)

    crepe.on((listener) => {
      listener.markdownUpdated((_ctx, editorMarkdown) => {
        if (disposed || applyingExternalValueRef.current) return
        if (!userEditObservedRef.current) return
        const nextMarkdown = joinNotebookEditorDocument(
          frontMatterRef.current,
          restoreNotebookMarkdownFromEditor(editorMarkdown),
        )
        latestDraftRef.current = nextMarkdown
        if (nextMarkdown === savedDraftRef.current) {
          setSaveState('saved')
          clearSaveTimer()
          return
        }
        setSaveState('dirty')
        scheduleSave()
      })
      listener.blur(() => {
        if (!disposed) void enqueueSave()
      })
    })

    const creation = crepe.create()
    void creation
      .then(() => {
        if (disposed) return
        crepeRef.current = crepe
      })
      .catch(() => {
        if (!disposed) setSaveState('error')
      })

    return () => {
      disposed = true
      mountedRef.current = false
      clearSaveTimer()
      if (latestDraftRef.current !== savedDraftRef.current) void enqueueSave(false)
      if (crepeRef.current === crepe) crepeRef.current = null
      void creation.then(() => crepe.destroy()).catch(() => undefined)
    }
  }, [clearSaveTimer, english, enqueueSave, scheduleSave])

  useEffect(() => {
    const crepe = crepeRef.current
    if (!crepe || markdown === latestDraftRef.current) return
    if (latestDraftRef.current !== savedDraftRef.current) return

    applyingExternalValueRef.current = true
    userEditObservedRef.current = false
    const externalDocument = splitNotebookEditorDocument(markdown)
    frontMatterRef.current = externalDocument.frontMatter
    latestDraftRef.current = markdown
    savedDraftRef.current = markdown
    crepe.editor.action(replaceAll(prepareNotebookMarkdownForEditor(externalDocument.body), true))
    applyingExternalValueRef.current = false
    setSaveState('saved')
  }, [markdown])

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLocaleLowerCase() !== 's') return
    event.preventDefault()
    const current = crepeRef.current?.getMarkdown()
    if (current !== undefined) {
      latestDraftRef.current = joinNotebookEditorDocument(
        frontMatterRef.current,
        restoreNotebookMarkdownFromEditor(current),
      )
    }
    void enqueueSave()
  }, [enqueueSave])

  const handleClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    if (!event.ctrlKey && !event.metaKey) return
    const target = event.target
    if (!(target instanceof Element)) return
    const link = target.closest<HTMLAnchorElement>('a[href^="#folumi-wiki-"]')
    const wikiTarget = notebookWikiTargetFromHref(link?.getAttribute('href') ?? '')
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
      onBeforeInput={() => { userEditObservedRef.current = true }}
      onCut={() => { userEditObservedRef.current = true }}
      onDrop={() => { userEditObservedRef.current = true }}
      onPaste={() => { userEditObservedRef.current = true }}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
    >
      <div ref={rootRef} className="min-h-[calc(100vh-205px)] flex-1" />
      <div
        className={`pointer-events-none sticky bottom-0 h-8 bg-gradient-to-t from-white via-white/90 to-transparent pt-2 text-right text-[11px] ${saveState === 'error' ? 'text-red-600' : 'text-gray-400'}`}
        aria-live="polite"
      >
        {statusLabel}
      </div>
    </div>
  )
}
