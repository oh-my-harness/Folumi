import { useId, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import type { Components } from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import rehypeAutolinkHeadings from 'rehype-autolink-headings'
import rehypeExternalLinks from 'rehype-external-links'
import rehypeKatex from 'rehype-katex'
import rehypeSlug from 'rehype-slug'
import remarkMath from 'remark-math'
import { normalizeLatexMathDelimiters } from '../markdownMath'

interface Props {
  text: string
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
  wikiLinkResolver?: (target: string) => SourceReference | undefined
  onWikiLinkCreate?: (target: string) => void
  disableHeadingLinks?: boolean
}

export function MarkdownMessage({ text, onSourceNavigate, wikiLinkResolver, onWikiLinkCreate, disableHeadingLinks = false }: Props) {
  const rawId = useId()
  const sourceListId = `source-refs-${rawId.replace(/[^a-zA-Z0-9_-]/g, '')}`
  const prepared = prepareMarkdownWithSourceReferences(text, sourceListId, wikiLinkResolver)
  const components: Components = {
    a({ href, children, ...props }) {
      if (href?.startsWith(`#${sourceListId}-item-`)) {
        const label = String(children)
        return (
          <a
            {...props}
            href={href}
            className="mx-0.5 align-baseline text-[0.82em] font-semibold text-blue-700 no-underline hover:text-blue-800 hover:underline"
            onClick={(event) => {
              event.preventDefault()
              document.getElementById(href.slice(1))?.scrollIntoView({ behavior: 'smooth', block: 'center' })
            }}
          >
            {label}
          </a>
        )
      }
      if (href?.startsWith('#notebook-wiki-')) {
        const rawTarget = decodeURIComponent(href.slice('#notebook-wiki-'.length))
        const reference = wikiLinkResolver?.(rawTarget)
        return (
          <button
            className="inline p-0 text-left font-medium text-blue-700 underline-offset-2 hover:text-blue-800 hover:underline"
            type="button"
            onClick={(event) => {
              event.preventDefault()
              if (reference?.target) onSourceNavigate?.(reference.target, reference)
              else onWikiLinkCreate?.(rawTarget)
            }}
          >
            {children}
          </button>
        )
      }
      if (disableHeadingLinks && href?.startsWith('#')) return <>{children}</>
      return (
        <a {...props} href={href}>
          {children}
        </a>
      )
    },
  }

  return (
    <div className="markdown-message text-sm">
      <ReactMarkdown
        skipHtml
        remarkPlugins={[remarkGfm, remarkBreaks, remarkMath]}
        rehypePlugins={[
          rehypeKatex,
          rehypeSlug,
          [rehypeAutolinkHeadings, { behavior: 'wrap' }],
          [rehypeExternalLinks, { target: '_blank', rel: ['nofollow', 'noopener', 'noreferrer'] }],
        ]}
        components={components}
      >
        {prepared.markdown}
      </ReactMarkdown>
      {prepared.references.length > 0 && (
        <SourceReferences id={sourceListId} references={prepared.references} onNavigate={onSourceNavigate} />
      )}
    </div>
  )
}

type SourceSurface = 'chat' | 'notebook' | 'quiz' | 'kb' | 'web' | 'unknown'

export type SourceTarget =
  | { type: 'chat'; sessionId: string; messageId?: string }
  | { type: 'notebook'; entryId: string }
  | { type: 'quiz'; quizId: string; questionId?: string }
  | { type: 'kb'; knowledgeBaseId: string; documentId: string; chunkId?: string }
  | { type: 'web'; url: string }

export interface SourceReference {
  id: string
  label: string
  raw: string
  surface: SourceSurface
  title?: string
  description?: string
  score?: number | null
  metadata?: SourceReferenceMetadata
  target?: SourceTarget
}

export interface SourceReferenceMetadata {
  documentName?: string
  documentId?: string
  chunkId?: string
  page?: string | number
  url?: string
  messageSnippet?: string
  stale?: boolean
  missingReason?: string
}

export function SourceReferences({
  id,
  references,
  onNavigate,
}: {
  id: string
  references: SourceReference[]
  onNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const [expanded, setExpanded] = useState(false)

  return (
    <section id={id} className="mt-4 border-t border-gray-200 pt-3 text-xs text-gray-500">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <button
          className="font-semibold text-gray-600 hover:text-blue-700"
          type="button"
          onClick={() => setExpanded((value) => !value)}
        >
          Sources {expanded ? 'v' : '>'}
        </button>
        <span className="text-gray-300">-</span>
        {references.map((reference) => (
          <SourceReferenceLink
            key={reference.id}
            id={`${id}-item-${safeReferenceId(reference.label)}`}
            reference={reference}
            onNavigate={onNavigate}
          />
        ))}
      </div>

      {expanded && (
        <ol className="m-0 mt-3 list-none space-y-2 p-0">
          {references.map((reference) => (
            <li key={`${reference.id}:detail`} className="scroll-mt-6">
              <div className="leading-5 text-gray-600">
                <SourceReferenceLink reference={reference} onNavigate={onNavigate} />
                {typeof reference.score === 'number' && (
                  <span className="ml-1 text-gray-400">score {reference.score.toFixed(4)}</span>
                )}
                <span className="mx-1 text-gray-300">-</span>
                <span>{sourceReferenceSubtitle(reference)}</span>
                {sourceReferenceBadges(reference).length > 0 && (
                  <span className="ml-1 text-gray-400">({sourceReferenceBadges(reference).join(', ')})</span>
                )}
                {!reference.target && (
                  <span className="ml-1 text-amber-700">
                    {reference.metadata?.missingReason || 'Source target unavailable'}
                  </span>
                )}
                {reference.description && (
                  <span className="mt-1 block max-w-4xl text-gray-500">{reference.description}</span>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
    </section>
  )

  return (
    <section id={id} className="mt-5 border-t border-gray-200 pt-4">
      <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-gray-500">Sources</div>
      <ol className="m-0 list-none space-y-2 p-0">
        {references.map((reference) => (
          <li
            key={reference.id}
            id={`${id}-item-${safeReferenceId(reference.label)}`}
            className="scroll-mt-6"
          >
            <button
              className={`flex w-full items-start gap-2 rounded-lg border border-blue-100 bg-white px-3 py-2 text-left text-sm text-gray-700 ${
                reference.target ? 'hover:border-blue-200 hover:bg-blue-50/50' : 'cursor-default'
              }`}
              type="button"
              disabled={!reference.target}
              onClick={() => {
                if (reference.target) onNavigate?.(reference.target, reference)
              }}
            >
              <span className="mt-0.5 inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-blue-600 px-1.5 text-xs font-semibold text-white">
                [{reference.label}]
              </span>
              <span className="min-w-0 flex-1">
                <span className="font-medium text-gray-900">{reference.title || sourceReferenceTitle(reference)}</span>
                {typeof reference.score === 'number' && (
                  <span className="ml-1 text-xs text-gray-400">{reference.score.toFixed(4)}</span>
                )}
                <span className="mx-1 text-gray-300">·</span>
                <span className="text-xs text-gray-500">{sourceReferenceSubtitle(reference)}</span>
                {sourceReferenceBadges(reference).length > 0 && (
                  <span className="mt-1 flex flex-wrap gap-1.5">
                    {sourceReferenceBadges(reference).map((badge) => (
                      <span key={badge} className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] font-medium text-gray-600">
                        {badge}
                      </span>
                    ))}
                  </span>
                )}
                {!reference.target && (
                  <span className="mt-1 block text-xs text-amber-700">
                    {reference.metadata?.missingReason || 'Source target unavailable'}
                  </span>
                )}
                {reference.description && (
                  <span className="mt-1 block text-xs leading-5 text-gray-600">{reference.description}</span>
                )}
              </span>
            </button>
          </li>
        ))}
      </ol>
    </section>
  )
}

function SourceReferenceLink({
  id,
  reference,
  onNavigate,
}: {
  id?: string
  reference: SourceReference
  onNavigate?: (target: SourceTarget, reference: SourceReference) => void
}) {
  const label = `[${reference.label}] ${reference.title || sourceReferenceTitle(reference)}`
  if (!reference.target) {
    return (
      <span id={id} className="text-gray-500">
        {label}
      </span>
    )
  }
  return (
    <button
      id={id}
      className="inline p-0 text-left font-medium text-blue-700 underline-offset-2 hover:text-blue-800 hover:underline"
      type="button"
      onClick={() => onNavigate?.(reference.target!, reference)}
    >
      {label}
    </button>
  )
}

function sourceReferenceTitle(reference: SourceReference) {
  return reference.metadata?.documentName || sourceSurfaceLabel(reference.surface)
}

function sourceReferenceSubtitle(reference: SourceReference) {
  if (reference.metadata?.url) return reference.metadata.url
  if (reference.metadata?.documentName) return reference.metadata.documentName
  return reference.raw
}

function sourceReferenceBadges(reference: SourceReference) {
  const badges: string[] = [sourceSurfaceLabel(reference.surface)]
  if (reference.metadata?.page != null) badges.push(`page ${reference.metadata.page}`)
  if (reference.metadata?.chunkId) badges.push(`chunk ${shortId(reference.metadata.chunkId)}`)
  if (reference.metadata?.documentId) badges.push(`doc ${shortId(reference.metadata.documentId)}`)
  if (reference.metadata?.stale) badges.push('stale')
  return badges
}

function shortId(value: string) {
  return value.length > 10 ? `${value.slice(0, 8)}...` : value
}

function prepareMarkdownWithSourceReferences(
  text: string,
  sourceListId: string,
  wikiLinkResolver?: (target: string) => SourceReference | undefined,
) {
  const withoutMarkers = stripInternalMemoryMarkers(text)
  const mathNormalized = normalizeLatexMathDelimiters(withoutMarkers)
  const { markdown, references } = extractFootnoteReferences(mathNormalized)
  const labels = new Set(references.map((reference) => reference.label))
  const sourceLinkedMarkdown = markdown.replace(/\[\^([^\]\s]+)\]/g, (match, label: string) => {
    if (!labels.has(label)) return match
    return `[${label}](#${sourceListId}-item-${safeReferenceId(label)})`
  })
  const linkedMarkdown = wikiLinkResolver ? linkNotebookWikiLinks(sourceLinkedMarkdown) : sourceLinkedMarkdown
  return { markdown: linkedMarkdown, references }
}

function linkNotebookWikiLinks(markdown: string) {
  return markdown.replace(/!?\[\[([^\]]+)\]\]/g, (match, inner: string) => {
    if (match.startsWith('!')) return match
    const [rawTarget, rawAlias] = inner.split('|')
    const target = rawTarget?.trim()
    if (!target) return match
    const alias = rawAlias?.trim() || target
    return `[${escapeMarkdownLinkText(alias)}](#notebook-wiki-${encodeURIComponent(target)})`
  })
}

function escapeMarkdownLinkText(value: string) {
  return value.replace(/\\/g, '\\\\').replace(/\]/g, '\\]')
}

function stripInternalMemoryMarkers(text: string) {
  return text.replace(/<!--\s*m_[a-zA-Z0-9_-]+\s*-->/g, '')
}

function extractFootnoteReferences(text: string) {
  const references: SourceReference[] = []
  const lines = text.split(/\r?\n/)
  const keptLines: string[] = []

  for (const line of lines) {
    const match = line.match(/^\s*\[\^([^\]\s]+)\]:\s*(.+?)\s*$/)
    if (match) {
      const label = match[1]
      const raw = match[2]
      if (label && raw) {
        references.push({
          id: `${label}:${raw}`,
          label,
          raw,
          surface: sourceSurfaceFromRaw(raw),
          target: sourceTargetFromRaw(raw),
        })
      }
      continue
    }
    keptLines.push(line)
  }

  return {
    markdown: stripTrailingFootnoteSeparator(keptLines).join('\n').trimEnd(),
    references,
  }
}

function stripTrailingFootnoteSeparator(lines: string[]) {
  const next = [...lines]
  while (next.length > 0 && next[next.length - 1]?.trim() === '') {
    next.pop()
  }
  if (/^\s*-{3,}\s*$/.test(next[next.length - 1] ?? '')) {
    next.pop()
  }
  return next
}

function sourceSurfaceFromRaw(raw: string): SourceSurface {
  const prefix = raw.split(':', 1)[0]?.toLowerCase()
  if (prefix === 'chat') return 'chat'
  if (prefix === 'notebook') return 'notebook'
  if (prefix === 'quiz') return 'quiz'
  if (prefix === 'kb') return 'kb'
  if (prefix === 'web' || raw.startsWith('http://') || raw.startsWith('https://')) return 'web'
  return 'unknown'
}

export function sourceTargetFromRaw(raw: string): SourceTarget | undefined {
  if (raw.startsWith('http://') || raw.startsWith('https://')) {
    return { type: 'web', url: raw }
  }

  const [prefix, ...parts] = raw.split(':')
  if (!prefix) return undefined
  const type = prefix.toLowerCase()

  if (type === 'web') {
    const url = parts.join(':')
    return url ? { type: 'web', url } : undefined
  }
  if (type === 'chat') {
    const [sessionId, messageId] = parts
    return sessionId ? { type: 'chat', sessionId, messageId } : undefined
  }
  if (type === 'notebook') {
    const [entryId] = parts
    return entryId ? { type: 'notebook', entryId } : undefined
  }
  if (type === 'quiz') {
    const [quizId, questionId] = parts
    return quizId ? { type: 'quiz', quizId, questionId } : undefined
  }
  if (type === 'kb') {
    const [knowledgeBaseId, documentId, chunkId] = parts
    return knowledgeBaseId && documentId ? { type: 'kb', knowledgeBaseId, documentId, chunkId } : undefined
  }

  return undefined
}

export function sourceSurfaceLabel(surface: SourceSurface) {
  if (surface === 'chat') return 'Chat'
  if (surface === 'notebook') return 'Notebook'
  if (surface === 'quiz') return 'Quiz'
  if (surface === 'kb') return 'Sources'
  if (surface === 'web') return 'Web'
  return 'Source'
}

function safeReferenceId(label: string) {
  return label.replace(/[^a-zA-Z0-9_-]/g, '-')
}
