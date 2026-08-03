export interface KnowledgeCitation {
  index: number
  source: string
  text: string
  kind: 'rag'
  title?: string
  kb?: string
  documentId?: string
  chunkId?: string
  rawSource?: string
}

export function knowledgeCitationsFromTrace(payload: Record<string, unknown>): KnowledgeCitation[] {
  if (
    payload.kind !== 'tool_result' ||
    payload.tool !== 'knowledge_read' ||
    payload.ok === false
  ) {
    return []
  }

  const details = asRecord(payload.details)
  const citation = asRecord(details?.citation)
  const reference = asRecord(citation?.reference)
  if (!details || !citation || !reference) return []

  const handle = nonEmptyString(citation.handle)
  const itemId = nonEmptyString(reference.item_id)
  const uri = nonEmptyString(details.uri)
  if (!handle || (!itemId && !uri)) return []

  const target = knowledgeTargetFromUri(uri)
  const source = target?.documentId || target?.sessionId || itemId || uri || 'Knowledge source'
  const title = target?.chunkId
    ? `${source} · ${target.chunkId}`
    : target?.sessionId
      ? `历史对话 · ${target.sessionId}`
      : source

  return [{
    index: 0,
    source,
    text: target?.sessionId
      ? `Runtime 历史检索来源 ${handle}`
      : `Verified course evidence ${handle}`,
    kind: 'rag',
    title,
    kb: target?.kb,
    documentId: target?.documentId,
    chunkId: target?.chunkId,
    rawSource: uri,
  }]
}

function knowledgeTargetFromUri(uri: string | undefined) {
  if (uri?.startsWith('chat:')) {
    const [, sessionId, messageId] = uri.split(':')
    if (!sessionId?.trim()) return undefined
    return { sessionId, messageId }
  }
  if (!uri?.startsWith('kb:')) return undefined
  const parts = uri.split(':')
  if (parts.length !== 4 || parts.some((part) => !part.trim())) return undefined
  return {
    kb: parts[1],
    documentId: parts[2],
    chunkId: parts[3],
  }
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}
