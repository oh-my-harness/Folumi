export interface TutorResourcePermissions {
  knowledge_base_ids: string[]
  notebook: boolean
  space: boolean
}

export const BUILT_IN_GUIDE_TUTOR_ID = 'general-tutor'

export interface TutorProfile {
  id: string
  name: string
  soul_markdown: string
  avatar?: string | null
  default_model_config_id?: string | null
  default_capability: string
  allowed_capabilities: string[]
  learner_memory_access: boolean
  resource_permissions: TutorResourcePermissions
  autonomous_memory: boolean
  built_in: boolean
  archived: boolean
  created_at: string
  updated_at: string
}

export interface TutorSummary {
  id: string
  name: string
  avatar?: string | null
  built_in?: boolean
  archived?: boolean
}

export interface TutorDraft {
  name: string
  soul_markdown: string
  default_model_config_id: string | null
  default_capability: string
  allowed_capabilities: string[]
  learner_memory_access: boolean
  autonomous_memory: boolean
  resource_permissions: TutorResourcePermissions
}

export type TutorMemoryKind = 'commitment' | 'open_loop' | 'lesson_plan' | 'reflection' | 'strategy'
export type TutorMemoryStatus = 'active' | 'resolved'

export interface TutorMemoryEntry {
  id: string
  tutor_id: string
  kind: TutorMemoryKind
  text: string
  status: TutorMemoryStatus
  next_action?: string | null
  due_at?: string | null
  source_session_id?: string | null
  source_message_id?: string | null
  resolution_note?: string | null
  created_at: string
  updated_at: string
  resolved_at?: string | null
}

export interface TutorMemoryDraft {
  kind: TutorMemoryKind
  text: string
  next_action?: string | null
}

export const tutorCapabilities = ['chat', 'quiz', 'research', 'organize'] as const

export function tutorSoulSummary(markdown: string) {
  const line = markdown
    .split(/\r?\n/)
    .map((item) => item.trim())
    .find((item) => item && !item.startsWith('#') && !item.startsWith('-'))
  return line ?? '尚未设置导师 Soul'
}

export async function fetchTutors(): Promise<TutorProfile[]> {
  const response = await fetch('/api/tutors')
  const data = await readJson(response)
  if (!response.ok) throw new Error(apiError(data, response.status))
  return Array.isArray(data.tutors) ? data.tutors.map(normalizeTutorProfile) : []
}

export async function createTutor(draft: TutorDraft): Promise<TutorProfile> {
  return mutateTutor('/api/tutors', 'POST', draft)
}

export async function updateTutor(id: string, draft: Partial<TutorDraft>): Promise<TutorProfile> {
  return mutateTutor(`/api/tutors/${encodeURIComponent(id)}`, 'PATCH', draft)
}

export async function archiveTutor(id: string): Promise<void> {
  const response = await fetch(`/api/tutors/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!response.ok) {
    const data = await readJson(response)
    throw new Error(apiError(data, response.status))
  }
}

export async function fetchTutorMemory(id: string, includeResolved = true): Promise<TutorMemoryEntry[]> {
  const response = await fetch(`/api/tutors/${encodeURIComponent(id)}/memory?include_resolved=${includeResolved}`)
  const data = await readJson(response)
  if (!response.ok) throw new Error(apiError(data, response.status))
  return Array.isArray(data.entries) ? data.entries as TutorMemoryEntry[] : []
}

export async function createTutorMemory(id: string, draft: TutorMemoryDraft): Promise<TutorMemoryEntry> {
  return mutateTutorMemory(`/api/tutors/${encodeURIComponent(id)}/memory`, 'POST', draft)
}

export async function updateTutorMemory(
  tutorId: string,
  entryId: string,
  draft: Partial<TutorMemoryDraft> & { status?: TutorMemoryStatus; resolution_note?: string | null },
): Promise<TutorMemoryEntry> {
  return mutateTutorMemory(
    `/api/tutors/${encodeURIComponent(tutorId)}/memory/${encodeURIComponent(entryId)}`,
    'PATCH',
    draft,
  )
}

export async function resolveTutorMemory(tutorId: string, entryId: string): Promise<TutorMemoryEntry> {
  return mutateTutorMemory(
    `/api/tutors/${encodeURIComponent(tutorId)}/memory/${encodeURIComponent(entryId)}/resolve`,
    'POST',
    {},
  )
}

export async function deleteTutorMemory(tutorId: string, entryId: string): Promise<void> {
  const response = await fetch(
    `/api/tutors/${encodeURIComponent(tutorId)}/memory/${encodeURIComponent(entryId)}`,
    { method: 'DELETE' },
  )
  if (!response.ok) {
    const data = await readJson(response)
    throw new Error(apiError(data, response.status))
  }
}

export async function resetTutorMemory(tutorId: string): Promise<void> {
  const response = await fetch(`/api/tutors/${encodeURIComponent(tutorId)}/reset-memory`, { method: 'POST' })
  if (!response.ok) {
    const data = await readJson(response)
    throw new Error(apiError(data, response.status))
  }
}

async function mutateTutorMemory(
  url: string,
  method: 'POST' | 'PATCH',
  body: unknown,
): Promise<TutorMemoryEntry> {
  const response = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const data = await readJson(response)
  if (!response.ok) throw new Error(apiError(data, response.status))
  return data as unknown as TutorMemoryEntry
}

async function mutateTutor(url: string, method: 'POST' | 'PATCH', body: unknown): Promise<TutorProfile> {
  const response = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const data = await readJson(response)
  if (!response.ok) throw new Error(apiError(data, response.status))
  assertTutorMutationContract(data)
  return normalizeTutorProfile(data)
}

async function readJson(response: Response): Promise<Record<string, unknown>> {
  return response.json().catch(() => ({})) as Promise<Record<string, unknown>>
}

function apiError(data: Record<string, unknown>, status: number) {
  return typeof data.error === 'string' ? data.error : `HTTP ${status}`
}

export function normalizeTutorProfile(value: unknown): TutorProfile {
  const profile = value && typeof value === 'object'
    ? value as Record<string, unknown>
    : {}
  const soul = typeof profile.soul_markdown === 'string' && profile.soul_markdown.trim()
    ? profile.soul_markdown
    : legacyTutorSoul(profile.role)
  const allowedCapabilities = Array.isArray(profile.allowed_capabilities)
    ? profile.allowed_capabilities.filter((capability): capability is string => (
        typeof capability === 'string' && capability !== 'deep_solve'
      ))
    : ['chat']
  const normalizedAllowed = allowedCapabilities.length > 0 ? allowedCapabilities : ['chat']
  const defaultCapability = typeof profile.default_capability === 'string'
    && normalizedAllowed.includes(profile.default_capability)
    ? profile.default_capability
    : 'chat'
  return {
    ...profile,
    soul_markdown: soul,
    default_capability: defaultCapability,
    allowed_capabilities: normalizedAllowed,
  } as unknown as TutorProfile
}

export function assertTutorMutationContract(value: Record<string, unknown>) {
  if (typeof value.soul_markdown !== 'string') {
    throw new Error('当前后端版本不支持导师 Soul，请重启 Folumi 后再保存。')
  }
}

function legacyTutorSoul(role: unknown) {
  const identity = typeof role === 'string' && role.trim()
    ? role.trim()
    : '请根据学习者的需要提供清晰、可靠的教学帮助。'
  return `# 核心身份\n\n${identity}`
}
