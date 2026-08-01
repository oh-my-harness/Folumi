import { BookOpen, NotebookPen } from 'lucide-react'
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

  return (
    <main className="flex min-h-0 flex-1 flex-col bg-white">
      <header className="shrink-0 border-b border-gray-200 px-6 pt-5">
        <div>
          <h1 className="text-xl font-semibold text-gray-950">
            {language === 'en-US' ? 'Knowledge Base' : '知识库'}
          </h1>
          <p className="mt-1 text-sm text-gray-500">
            {language === 'en-US'
              ? 'Keep source documents and your own notes in one place.'
              : '在同一个地方管理来源文档和你自己的笔记。'}
          </p>
        </div>
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
          <KnowledgePage settings={settings} onChanged={onChanged} focusTarget={knowledgeFocusTarget} />
        ) : (
          <SpacePage mode="notebook" embedded focusTarget={noteFocusTarget} onSourceNavigate={onSourceNavigate} />
        )}
      </div>
    </main>
  )
}
