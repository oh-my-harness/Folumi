export interface SessionRunItem {
  id: string
  activeRun?: unknown | null
}

export function isCurrentSessionEvent(sourceSessionId: string, currentSessionId: string | null) {
  return sourceSessionId === currentSessionId
}

export function isLatestSessionLoad(
  requestVersion: number,
  currentVersion: number,
  requestedSessionId: string,
  currentSessionId: string | null,
) {
  return requestVersion === currentVersion && requestedSessionId === currentSessionId
}

export function isLatestSessionHydration(
  requestSelectionVersion: number,
  currentSelectionVersion: number,
  requestHydrationVersion: number,
  currentHydrationVersion: number,
  requestedSessionId: string,
  currentSessionId: string | null,
) {
  return isLatestSessionLoad(
    requestSelectionVersion,
    currentSelectionVersion,
    requestedSessionId,
    currentSessionId,
  ) && requestHydrationVersion === currentHydrationVersion
}

export function reconcileSessionRunState<T extends SessionRunItem>(
  current: T[],
  incoming: SessionRunItem[],
): T[] {
  const runsBySession = new Map(incoming.map((session) => [session.id, session.activeRun ?? null]))
  return current.map((session) => (
    runsBySession.has(session.id)
      ? { ...session, activeRun: runsBySession.get(session.id) }
      : session
  ))
}

export interface SessionMessageItem {
  role: string
  text: string
}

function messageKey(message: SessionMessageItem) {
  return `${message.role}\u0000${message.text}`
}

/**
 * Durable history is authoritative, but messages received while history was
 * loading must survive hydration. Matching messages are consumed as a
 * multiset so repeated turns remain distinct.
 */
export function reconcileSessionMessages<T extends SessionMessageItem>(
  restored: T[],
  live: T[],
): T[] {
  const remainingRestored = new Map<string, number>()
  for (const message of restored) {
    const key = messageKey(message)
    remainingRestored.set(key, (remainingRestored.get(key) ?? 0) + 1)
  }

  const merged = [...restored]
  for (const message of live) {
    const key = messageKey(message)
    const remaining = remainingRestored.get(key) ?? 0
    if (remaining > 0) {
      remainingRestored.set(key, remaining - 1)
    } else {
      merged.push(message)
    }
  }
  return merged
}

/** Avoid duplicating the final answer when a completed stream snapshot is replayed. */
export function appendCompletedSessionMessage<T extends SessionMessageItem>(
  messages: T[],
  message: T,
): T[] {
  const lastDurable = [...messages].reverse().find((item) => item.role !== 'status')
  if (lastDurable?.role === message.role && lastDurable.text === message.text) {
    return messages
  }
  return [...messages, message]
}

/**
 * A non-chunk stream event carries the runtime's canonical final answer.
 * Prefer it over provisional deltas, which can later be classified as
 * tool-call progress rather than a durable assistant answer.
 */
export function completedStreamText(streamedText: string, canonicalText: string) {
  return canonicalText || streamedText
}
