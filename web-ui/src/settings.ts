import type { UiLanguage } from './i18n'

export type LlmProvider = 'anthropic' | 'openai'
export type EmbeddingProvider = 'openai'
export type ThemeId = 'cool-light' | 'graphite-dark'
export type SearchProvider =
  | 'duckduckgo'
  | 'bing'
  | 'brave'
  | 'tavily'
  | 'serper'
  | 'serpapi'
  | 'exa'

export const CURRENT_ONBOARDING_VERSION = 1

export interface LlmModelConfig {
  id: string
  name: string
  provider: LlmProvider
  model: string
  apiKey: string
  baseUrl: string
  chatPath: string
  contextWindowTokens: number
}

export interface EmbeddingModelConfig {
  id: string
  name: string
  provider: EmbeddingProvider
  baseUrl: string
  embeddingsPath: string
  apiKey: string
  model: string
  dimensions: number
  sendDimensions: boolean
}

export interface SearchConfig {
  id: string
  name: string
  provider: SearchProvider
  baseUrl: string
  apiKey: string
  maxResults: number
  fetchTimeoutSecs: number
  maxFetchChars: number
}

export interface LlmSettings {
  assistantName: string
  assistantInstructions: string
  memoryEnabled: boolean
  language: UiLanguage
  theme: ThemeId
  provider: LlmProvider
  model: string
  apiKey: string
  baseUrl: string
  chatPath: string
  budgetLimitUsd: number
  requireApproval: boolean
  llmConfigs: LlmModelConfig[]
  activeLlmConfigId: string | null
  embeddingConfigs: EmbeddingModelConfig[]
  activeEmbeddingConfigId: string | null
  searchConfigs: SearchConfig[]
  activeSearchConfigId: string | null
  onboardingVersion: number
  onboardingCompleted: boolean
  dismissedContextHints: string[]
}

export const DEFAULT_CONTEXT_WINDOW_TOKENS = 128000

export const defaultLlmSettings: LlmSettings = {
  assistantName: 'Folumi Assistant',
  assistantInstructions: '',
  memoryEnabled: true,
  language: 'zh-CN',
  theme: 'cool-light',
  provider: 'openai',
  model: 'deepseek-v4-flash',
  apiKey: '',
  baseUrl: 'https://api.deepseek.com',
  chatPath: '/chat/completions',
  budgetLimitUsd: 2,
  requireApproval: false,
  llmConfigs: [],
  activeLlmConfigId: null,
  embeddingConfigs: [],
  activeEmbeddingConfigId: null,
  searchConfigs: [],
  activeSearchConfigId: null,
  onboardingVersion: CURRENT_ONBOARDING_VERSION,
  onboardingCompleted: false,
  dismissedContextHints: [],
}

const SETTINGS_STORAGE_KEY = 'tutor.llmSettings'

export function loadLlmSettings(): LlmSettings {
  const raw = localStorage.getItem(SETTINGS_STORAGE_KEY)
  if (!raw) return defaultLlmSettings

  try {
    return normalizeLlmSettings(JSON.parse(raw) as Partial<LlmSettings>)
  } catch {
    return defaultLlmSettings
  }
}

export function saveLlmSettings(settings: LlmSettings) {
  localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings))
}

export function hasLocalLlmSettings(): boolean {
  return Boolean(localStorage.getItem(SETTINGS_STORAGE_KEY))
}

export async function loadStoredLlmSettings(): Promise<LlmSettings | null> {
  const response = await fetch('/api/settings')
  if (!response.ok) {
    throw new Error(`failed to load settings: HTTP ${response.status}`)
  }
  const payload = await response.json() as { settings?: unknown }
  if (!hasSettingsPayload(payload.settings)) return null
  return normalizeLlmSettings(payload.settings as Partial<LlmSettings>)
}

export async function saveStoredLlmSettings(settings: LlmSettings): Promise<void> {
  const response = await fetch('/api/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!response.ok) {
    throw new Error(`failed to save settings: HTTP ${response.status}`)
  }
}

export function settingsForSession(
  settings: LlmSettings,
  configId: string | null = settings.activeLlmConfigId,
) {
  const config = configId === null
    ? null
    : settings.llmConfigs.find((item) => item.id === configId) ?? null
  return {
    provider: config?.provider ?? settings.provider,
    model: (config?.model ?? settings.model).trim(),
    api_key: (config?.apiKey ?? settings.apiKey).trim(),
    base_url: (config?.baseUrl ?? settings.baseUrl).trim() || null,
    chat_path: (config?.chatPath ?? settings.chatPath).trim() || null,
    context_window_tokens: Number(config?.contextWindowTokens || DEFAULT_CONTEXT_WINDOW_TOKENS),
    budget_limit_usd: settings.budgetLimitUsd,
    require_approval: settings.requireApproval,
  }
}

export function hasUsableLlmConfig(settings: LlmSettings): boolean {
  const config = activeLlmConfig(settings)
  if (!config) return false
  return Boolean(config.model.trim() && config.apiKey.trim() && config.baseUrl.trim())
}

export function shouldShowOnboarding(
  settings: LlmSettings,
  currentVersion = CURRENT_ONBOARDING_VERSION,
): boolean {
  return !settings.onboardingCompleted || settings.onboardingVersion < currentVersion
}

export function completeOnboardingSettings(
  settings: LlmSettings,
  currentVersion = CURRENT_ONBOARDING_VERSION,
): LlmSettings {
  return {
    ...settings,
    onboardingVersion: currentVersion,
    onboardingCompleted: true,
  }
}

export async function testLlmConnection(config: LlmModelConfig): Promise<Record<string, unknown>> {
  const response = await fetch('/api/settings/test/llm', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      provider: config.provider,
      model: config.model,
      api_key: config.apiKey,
      base_url: config.baseUrl,
      chat_path: config.chatPath,
    }),
  })
  const payload = await response.json().catch(() => ({})) as Record<string, unknown>
  if (!response.ok) {
    throw new Error(typeof payload.error === 'string' ? payload.error : 'Model test failed')
  }
  return payload
}

export function searchForSession(settings: LlmSettings) {
  const config = activeSearchConfig(settings)
  if (!config) return null
  return {
    provider: config.provider,
    base_url: config.baseUrl.trim(),
    api_key: config.apiKey.trim() || null,
    max_results: Number(config.maxResults || 5),
    fetch_timeout_secs: Number(config.fetchTimeoutSecs || 12),
    max_fetch_chars: Number(config.maxFetchChars || 12000),
  }
}

export function activeLlmConfig(settings: LlmSettings): LlmModelConfig | null {
  return settings.llmConfigs.find((config) => config.id === settings.activeLlmConfigId) ?? null
}

export function createLlmConfig(provider: LlmProvider = 'openai'): LlmModelConfig {
  const preset = llmProviderPreset(provider)
  return {
    id: crypto.randomUUID(),
    name: preset.label,
    provider,
    model: preset.model,
    apiKey: '',
    baseUrl: preset.baseUrl,
    chatPath: preset.chatPath,
    contextWindowTokens: preset.contextWindowTokens,
  }
}

export function activeEmbeddingConfig(settings: LlmSettings): EmbeddingModelConfig | null {
  return settings.embeddingConfigs.find((config) => config.id === settings.activeEmbeddingConfigId) ?? null
}

export function createEmbeddingConfig(): EmbeddingModelConfig {
  return {
    id: crypto.randomUUID(),
    name: 'OpenAI',
    provider: 'openai',
    baseUrl: 'https://api.openai.com',
    embeddingsPath: '/v1/embeddings',
    apiKey: '',
    model: 'text-embedding-3-small',
    dimensions: 1536,
    sendDimensions: false,
  }
}

export function activeSearchConfig(settings: LlmSettings): SearchConfig | null {
  return settings.searchConfigs.find((config) => config.id === settings.activeSearchConfigId) ?? null
}

export function createSearchConfig(): SearchConfig {
  return {
    id: crypto.randomUUID(),
    name: 'DuckDuckGo',
    provider: 'duckduckgo',
    baseUrl: 'https://duckduckgo.com/html/',
    apiKey: '',
    maxResults: 5,
    fetchTimeoutSecs: 12,
    maxFetchChars: 12000,
  }
}

export function searchProviderPreset(provider: SearchProvider) {
  if (provider === 'bing') {
    return {
      name: 'Bing',
      baseUrl: 'https://www.bing.com/search',
    }
  }
  if (provider === 'brave') {
    return {
      name: 'Brave',
      baseUrl: 'https://api.search.brave.com/res/v1/web/search',
    }
  }
  if (provider === 'tavily') {
    return {
      name: 'Tavily',
      baseUrl: 'https://api.tavily.com/search',
    }
  }
  if (provider === 'serper') {
    return {
      name: 'Serper',
      baseUrl: 'https://google.serper.dev/search',
    }
  }
  if (provider === 'serpapi') {
    return {
      name: 'SerpAPI',
      baseUrl: 'https://serpapi.com/search.json',
    }
  }
  if (provider === 'exa') {
    return {
      name: 'Exa',
      baseUrl: 'https://api.exa.ai/search',
    }
  }
  return {
    name: 'DuckDuckGo',
    baseUrl: 'https://duckduckgo.com/html/',
  }
}

export function embeddingForSession(config: EmbeddingModelConfig) {
  return {
    provider: config.provider,
    model: config.model.trim(),
    api_key: config.apiKey.trim(),
    base_url: config.baseUrl.trim() || null,
    embeddings_path: config.embeddingsPath.trim() || null,
    dimensions: Number(config.dimensions || 0) || null,
    send_dimensions: config.sendDimensions,
  }
}

export function llmProviderPreset(provider: LlmProvider) {
  if (provider === 'anthropic') {
    return {
      label: 'Anthropic',
      model: 'claude-haiku-4-5-20251001',
      baseUrl: 'https://api.anthropic.com',
      chatPath: '',
      contextWindowTokens: 200000,
    }
  }

  return {
    label: 'OpenAI-compatible',
    model: 'gpt-4o-mini',
    baseUrl: 'https://api.openai.com',
    chatPath: '/v1/chat/completions',
    contextWindowTokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
  }
}

function normalizeLlmConfigs(value: unknown, legacy: Partial<LlmSettings>): LlmModelConfig[] {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeLlmConfig(item))
  }

  if (legacy.model || legacy.apiKey || legacy.provider || legacy.baseUrl || legacy.chatPath) {
    const legacyProvider = legacy.provider as unknown
    const provider = normalizeLlmProvider(legacyProvider)
    const legacyPresetLabel =
      legacyProvider === 'deepseek' ? 'DeepSeek' : llmProviderPreset(provider).label

    return [
      {
        id: crypto.randomUUID(),
        name: legacyPresetLabel,
        provider,
        model: typeof legacy.model === 'string' ? legacy.model : defaultLlmSettings.model,
        apiKey: typeof legacy.apiKey === 'string' ? legacy.apiKey : '',
        baseUrl: typeof legacy.baseUrl === 'string' ? legacy.baseUrl : defaultLlmSettings.baseUrl,
        chatPath: typeof legacy.chatPath === 'string' ? legacy.chatPath : defaultLlmSettings.chatPath,
        contextWindowTokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
      },
    ]
  }

  return []
}

function normalizeLlmSettings(parsed: Partial<LlmSettings>): LlmSettings {
  const llmConfigs = normalizeLlmConfigs(parsed.llmConfigs, parsed)
  const activeLlmConfigId = normalizeActiveConfigId(parsed.activeLlmConfigId, llmConfigs)
  const activeLlmConfig = llmConfigs.find((config) => config.id === activeLlmConfigId) ?? null
  const embeddingConfigs = normalizeEmbeddingConfigs(parsed.embeddingConfigs)
  const searchConfigs = normalizeSearchConfigs(parsed.searchConfigs)

  return {
    ...defaultLlmSettings,
    ...parsed,
    ...(activeLlmConfig ? llmConfigToLegacyFields(activeLlmConfig) : {}),
    provider: activeLlmConfig
      ? activeLlmConfig.provider
      : normalizeLlmProvider((parsed as { provider?: unknown }).provider),
    budgetLimitUsd: Number(parsed.budgetLimitUsd ?? defaultLlmSettings.budgetLimitUsd),
    requireApproval: Boolean(parsed.requireApproval),
    assistantName: typeof parsed.assistantName === 'string' && parsed.assistantName.trim()
      ? parsed.assistantName.trim()
      : defaultLlmSettings.assistantName,
    assistantInstructions: typeof parsed.assistantInstructions === 'string'
      ? parsed.assistantInstructions
      : '',
    memoryEnabled: parsed.memoryEnabled !== false,
    language: normalizeUiLanguage((parsed as { language?: unknown }).language),
    theme: normalizeTheme((parsed as { theme?: unknown }).theme),
    llmConfigs,
    activeLlmConfigId,
    embeddingConfigs,
    activeEmbeddingConfigId: normalizeActiveConfigId(parsed.activeEmbeddingConfigId, embeddingConfigs),
    searchConfigs,
    activeSearchConfigId: normalizeActiveConfigId(parsed.activeSearchConfigId, searchConfigs),
    onboardingVersion: normalizeNonNegativeInteger(
      parsed.onboardingVersion,
      defaultLlmSettings.onboardingVersion,
    ),
    onboardingCompleted: Boolean(parsed.onboardingCompleted),
    dismissedContextHints: normalizeStringArray(parsed.dismissedContextHints),
  }
}

export function normalizeTheme(value: unknown): ThemeId {
  return value === 'graphite-dark' ? 'graphite-dark' : 'cool-light'
}

export function settingsRequireSessionReset(current: LlmSettings, next: LlmSettings): boolean {
  return JSON.stringify(runtimeRelevantSettings(current)) !== JSON.stringify(runtimeRelevantSettings(next))
}

function runtimeRelevantSettings(settings: LlmSettings) {
  const {
    theme: _theme,
    language: _language,
    onboardingVersion: _onboardingVersion,
    onboardingCompleted: _onboardingCompleted,
    dismissedContextHints: _dismissedContextHints,
    ...runtimeSettings
  } = settings
  return runtimeSettings
}

function normalizeUiLanguage(value: unknown): UiLanguage {
  return value === 'en-US' ? 'en-US' : 'zh-CN'
}

function hasSettingsPayload(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  return Object.keys(value).length > 0
}

function normalizeLlmConfig(value: unknown): LlmModelConfig {
  const config = value as Partial<LlmModelConfig>
  const provider = normalizeLlmProvider(config.provider)
  const preset = llmProviderPreset(provider)
  return {
    id: typeof config.id === 'string' && config.id ? config.id : crypto.randomUUID(),
    name: typeof config.name === 'string' && config.name ? config.name : preset.label,
    provider,
    model: typeof config.model === 'string' ? config.model : preset.model,
    apiKey: typeof config.apiKey === 'string' ? config.apiKey : '',
    baseUrl: typeof config.baseUrl === 'string' ? config.baseUrl : preset.baseUrl,
    chatPath: typeof config.chatPath === 'string' ? config.chatPath : preset.chatPath,
    contextWindowTokens: normalizePositiveNumber(config.contextWindowTokens, preset.contextWindowTokens),
  }
}

function normalizePositiveNumber(value: unknown, fallback: number): number {
  const numberValue = Number(value)
  return Number.isFinite(numberValue) && numberValue > 0 ? Math.round(numberValue) : fallback
}

function normalizeNonNegativeInteger(value: unknown, fallback: number): number {
  const numberValue = Number(value)
  return Number.isFinite(numberValue) && numberValue >= 0 ? Math.round(numberValue) : fallback
}

function normalizeStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return [...new Set(value.filter((item): item is string => typeof item === 'string' && Boolean(item)))]
}

function normalizeLlmProvider(value: unknown): LlmProvider {
  if (value === 'anthropic') return 'anthropic'
  return 'openai'
}

function normalizeEmbeddingConfigs(value: unknown): EmbeddingModelConfig[] {
  if (!Array.isArray(value)) return []

  return value.map((item) => {
    const config = item as Partial<EmbeddingModelConfig>
    return {
      id: typeof config.id === 'string' && config.id ? config.id : crypto.randomUUID(),
      name: typeof config.name === 'string' && config.name ? config.name : 'OpenAI',
      provider: 'openai',
      baseUrl: typeof config.baseUrl === 'string' ? config.baseUrl : 'https://api.openai.com',
      embeddingsPath: typeof config.embeddingsPath === 'string' ? config.embeddingsPath : '/v1/embeddings',
      apiKey: typeof config.apiKey === 'string' ? config.apiKey : '',
      model: typeof config.model === 'string' ? config.model : 'text-embedding-3-small',
      dimensions: Number(config.dimensions || 1536),
      sendDimensions: Boolean(config.sendDimensions),
    }
  })
}

function normalizeSearchConfigs(value: unknown): SearchConfig[] {
  if (!Array.isArray(value)) return []

  return value.map((item) => {
    const config = item as Partial<SearchConfig>
    const provider = normalizeSearchProvider(config.provider)
    const preset = searchProviderPreset(provider)
    return {
      id: typeof config.id === 'string' && config.id ? config.id : crypto.randomUUID(),
      name: typeof config.name === 'string' && config.name ? config.name : preset.name,
      provider,
      baseUrl: typeof config.baseUrl === 'string' ? config.baseUrl : preset.baseUrl,
      apiKey: typeof config.apiKey === 'string' ? config.apiKey : '',
      maxResults: Number(config.maxResults || 5),
      fetchTimeoutSecs: normalizePositiveNumber(config.fetchTimeoutSecs, 12),
      maxFetchChars: normalizePositiveNumber(config.maxFetchChars, 12000),
    }
  })
}

function normalizeSearchProvider(value: unknown): SearchProvider {
  if (value === 'bing') return 'bing'
  if (value === 'brave') return 'brave'
  if (value === 'tavily') return 'tavily'
  if (value === 'serper') return 'serper'
  if (value === 'serpapi') return 'serpapi'
  if (value === 'exa') return 'exa'
  return 'duckduckgo'
}

function normalizeActiveConfigId<T extends { id: string }>(value: unknown, configs: T[]): string | null {
  if (typeof value !== 'string') return configs[0]?.id ?? null
  return configs.some((config) => config.id === value) ? value : configs[0]?.id ?? null
}

function llmConfigToLegacyFields(config: LlmModelConfig) {
  return {
    provider: config.provider,
    model: config.model,
    apiKey: config.apiKey,
    baseUrl: config.baseUrl,
    chatPath: config.chatPath,
  }
}
