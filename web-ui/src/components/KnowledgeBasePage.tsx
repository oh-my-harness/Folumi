import { useEffect, useState } from 'react'
import { BookOpen, FileText, NotebookPen, Search, X } from 'lucide-react'
import type { LlmSettings } from '../settings'
import type { SourceReference, SourceTarget } from './MarkdownMessage'
import { KnowledgePage } from './KnowledgePage'
import { SpacePage } from './SpacePage'

export type KnowledgeSection = 'sources' | 'notes'

interface Props {
  settings: LlmSettings
  section: KnowledgeSection
  onSectionChange: (section: KnowledgeSection) => void
  onChanged?: () => void
  knowledgeFocusTarget?: Extract<SourceTarget, { type: 'kb' }> | null
  noteFocusTarget?: Extract<SourceTarget, { type: 'notebook' }> | null
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}

type SearchFilter = 'all' | 'source' | 'note'

interface LibrarySearchHit {
  id: string
  type: 'source' | 'note'
  title: string
  snippet: string
  location: string
  knowledge_base_id?: string
  document_id?: string
  note_id?: string
}

const sections = [
  { key: 'sources' as const, label: 'Sources', description: 'Imported documents and searchable indexes', icon: BookOpen },
  { key: 'notes' as const, label: 'Notes', description: 'Your editable Markdown knowledge', icon: NotebookPen },
]

export function KnowledgeBasePage({
  settings,
  section,
  onSectionChange,
  onChanged,
  knowledgeFocusTarget,
  noteFocusTarget,
  onSourceNavigate,
}: Props) {
  const language = settings.language
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<SearchFilter>('all')
  const [results, setResults] = useState<LibrarySearchHit[]>([])
  const [searching, setSearching] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchError, setSearchError] = useState('')
  const [selectedSource, setSelectedSource] = useState<Extract<SourceTarget, { type: 'kb' }> | null>(null)
  const [selectedNote, setSelectedNote] = useState<Extract<SourceTarget, { type: 'notebook' }> | null>(null)

  useEffect(() => setSelectedSource(null), [knowledgeFocusTarget])
  useEffect(() => setSelectedNote(null), [noteFocusTarget])

  const searchLibrary = async () => {
    const trimmed = query.trim()
    if (!trimmed || searching) return
    setSearching(true)
    setSearchError('')
    try {
      const params = new URLSearchParams({ q: trimmed, limit: '40' })
      if (filter !== 'all') params.set('type', filter)
      const response = await fetch(`/api/library/search?${params.toString()}`)
      const data = await response.json().catch(() => ({})) as { hits?: LibrarySearchHit[]; warnings?: string[]; error?: string }
      if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`)
      setResults(data.hits ?? [])
      setSearchError((data.warnings ?? []).join(' · '))
      setSearchOpen(true)
    } catch (error) {
      setResults([])
      setSearchError(error instanceof Error ? error.message : String(error))
      setSearchOpen(true)
    } finally {
      setSearching(false)
    }
  }

  const openResult = (hit: LibrarySearchHit) => {
    if (hit.type === 'source' && hit.knowledge_base_id && hit.document_id) {
      setSelectedSource({
        type: 'kb',
        knowledgeBaseId: hit.knowledge_base_id,
        documentId: hit.document_id,
      })
      onSectionChange('sources')
    } else if (hit.type === 'note' && hit.note_id) {
      setSelectedNote({ type: 'notebook', entryId: hit.note_id })
      onSectionChange('notes')
    }
    setSearchOpen(false)
  }

  return (
    <main className="flex min-h-0 flex-1 flex-col bg-white">
      <header className="relative shrink-0 border-b border-gray-200 px-6 pt-5">
        <div className="flex flex-wrap items-start gap-4">
          <div className="min-w-0 flex-1">
            <h1 className="text-xl font-semibold text-gray-950">
              {language === 'en-US' ? 'Knowledge Base' : '知识库'}
            </h1>
            <p className="mt-1 text-sm text-gray-500">
              {language === 'en-US'
                ? 'Keep source documents and your own notes in one place.'
                : '在同一个地方管理来源文档和你自己的笔记。'}
            </p>
          </div>
          <form
            className="relative flex w-full max-w-xl items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              void searchLibrary()
            }}
          >
            <div className="flex min-w-0 flex-1 items-center rounded-lg border border-gray-200 bg-gray-50 px-3 focus-within:border-blue-300 focus-within:bg-white">
              <Search size={16} className="shrink-0 text-gray-400" />
              <input
                className="h-10 min-w-0 flex-1 bg-transparent px-2 text-sm outline-none"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={language === 'en-US' ? 'Search Sources and Notes…' : '搜索来源和笔记…'}
              />
              {query && <button type="button" className="text-gray-400 hover:text-gray-700" aria-label="Clear search" onClick={() => { setQuery(''); setSearchOpen(false) }}><X size={15} /></button>}
            </div>
            <select className="h-10 rounded-lg border border-gray-200 bg-white px-2 text-sm text-gray-600" value={filter} onChange={(event) => setFilter(event.target.value as SearchFilter)}>
              <option value="all">{language === 'en-US' ? 'All' : '全部'}</option>
              <option value="source">{language === 'en-US' ? 'Sources' : '来源'}</option>
              <option value="note">{language === 'en-US' ? 'Notes' : '笔记'}</option>
            </select>
            <button type="submit" className="h-10 rounded-lg bg-blue-600 px-4 text-sm font-medium text-white disabled:opacity-50" disabled={!query.trim() || searching}>{searching ? '…' : language === 'en-US' ? 'Search' : '搜索'}</button>
          </form>
        </div>
        {searchOpen && (
          <div className="absolute right-6 top-[4.5rem] z-40 max-h-[min(28rem,60vh)] w-[min(40rem,calc(100vw-5rem))] overflow-y-auto rounded-lg border border-gray-200 bg-white p-2 shadow-2xl">
            <div className="flex items-center px-2 py-1 text-xs text-gray-500">
              <span>{searchError || (language === 'en-US' ? `${results.length} result(s)` : `${results.length} 条结果`)}</span>
              <button type="button" className="ml-auto rounded p-1 hover:bg-gray-100" aria-label="Close results" onClick={() => setSearchOpen(false)}><X size={15} /></button>
            </div>
            {!searchError && results.length === 0 && <div className="px-3 py-8 text-center text-sm text-gray-400">{language === 'en-US' ? 'No matching knowledge.' : '没有匹配的知识。'}</div>}
            {results.map((hit) => (
              <button key={hit.id} type="button" className="flex w-full gap-3 rounded-md px-3 py-3 text-left hover:bg-blue-50" onClick={() => openResult(hit)}>
                <span className={`mt-0.5 ${hit.type === 'source' ? 'text-blue-600' : 'text-amber-600'}`}>{hit.type === 'source' ? <FileText size={18} /> : <NotebookPen size={18} />}</span>
                <span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium text-gray-900">{hit.title}</span><span className="mt-0.5 block text-xs text-gray-500">{hit.location}</span><span className="mt-1 block line-clamp-2 text-xs leading-5 text-gray-600">{hit.snippet}</span></span>
              </button>
            ))}
          </div>
        )}
        <nav className="mt-5 flex gap-6" aria-label={language === 'en-US' ? 'Knowledge sections' : '知识库分类'}>
          {sections.map((item) => {
            const Icon = item.icon
            const active = section === item.key
            const label = language === 'en-US'
              ? item.label
              : item.key === 'sources' ? '来源' : '笔记'
            const description = language === 'en-US'
              ? item.description
              : item.key === 'sources' ? '导入的文档与可检索索引' : '可编辑的 Markdown 知识'
            return (
              <button
                key={item.key}
                type="button"
                className={`flex items-center gap-2 border-b-2 pb-3 text-left ${
                  active ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-900'
                }`}
                onClick={() => onSectionChange(item.key)}
              >
                <Icon size={17} />
                <span>
                  <span className="block text-sm font-medium">{label}</span>
                  <span className="hidden text-xs font-normal text-gray-400 lg:block">{description}</span>
                </span>
              </button>
            )
          })}
        </nav>
      </header>

      <div className="min-h-0 flex-1 overflow-hidden">
        {section === 'sources' ? (
          <KnowledgePage settings={settings} onChanged={onChanged} focusTarget={selectedSource ?? knowledgeFocusTarget} />
        ) : (
          <SpacePage mode="notebook" embedded focusTarget={selectedNote ?? noteFocusTarget} onSourceNavigate={onSourceNavigate} />
        )}
      </div>
    </main>
  )
}
