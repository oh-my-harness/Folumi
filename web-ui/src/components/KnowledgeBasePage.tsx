import type { LlmSettings } from '../settings'
import type { SourceTarget } from './MarkdownMessage'
import { KnowledgePage } from './KnowledgePage'

interface Props {
  settings: LlmSettings
  onChanged?: () => void
  focusTarget?: Extract<SourceTarget, { type: 'kb' }> | null
}

export function KnowledgeBasePage({ settings, onChanged, focusTarget }: Props) {
  return <KnowledgePage settings={settings} onChanged={onChanged} focusTarget={focusTarget} />
}
