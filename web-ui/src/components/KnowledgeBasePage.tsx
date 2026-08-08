import type { LlmSettings } from '../settings'
import type { SourceTarget } from './MarkdownMessage'
import { KnowledgePage } from './KnowledgePage'

interface Props {
  settings: LlmSettings
  onChanged?: () => void
  focusTarget?: Extract<SourceTarget, { type: 'kb' }> | null
  onConfigureEmbedding: () => void
}

export function KnowledgeBasePage({ settings, onChanged, focusTarget, onConfigureEmbedding }: Props) {
  return (
    <KnowledgePage
      settings={settings}
      onChanged={onChanged}
      focusTarget={focusTarget}
      onConfigureEmbedding={onConfigureEmbedding}
    />
  )
}
