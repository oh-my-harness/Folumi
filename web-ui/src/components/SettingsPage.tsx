import { useEffect, useState, type ReactNode } from 'react'
import {
  Activity,
  BookMarked,
  Brain,
  Check,
  CircleHelp,
  Database,
  FolderOpen,
  Globe2,
  Palette,
  Plus,
  Moon,
  SlidersHorizontal,
  Sun,
  Trash2,
  type LucideIcon,
} from 'lucide-react'
import {
  createEmbeddingConfig,
  createLlmConfig,
  createSearchConfig,
  llmProviderPreset,
  searchProviderPreset,
  testLlmConnection,
} from '../settings'
import { useI18n, type TranslationKey, type UiLanguage } from '../i18n'
import type {
  EmbeddingModelConfig,
  LlmModelConfig,
  LlmProvider,
  LlmSettings,
  SearchConfig,
  SearchProvider,
  ThemeId,
} from '../settings'
import { chooseDesktopDirectory, getDesktopDataDir, openDesktopDataDir } from '../api'
import type { ProductGuideDestination } from '../productGuide'
import { ProductGuide } from './ProductGuide'

interface Props {
  settings: LlmSettings
  activeTab: SettingsTab
  onTabChange: (tab: SettingsTab) => void
  onChange: (settings: LlmSettings) => void
  onOpenOnboarding: () => void
  onGuideNavigate: (destination: ProductGuideDestination) => void
  onStartGuideAssistant: () => void
}

const providerOptions: { value: LlmProvider; label: string; description: string }[] = [
  {
    value: 'openai',
    label: 'OpenAI-compatible',
    description: '适用于 OpenAI、DeepSeek、通义、硅基流动等 /chat/completions 接口。',
  },
  {
    value: 'anthropic',
    label: 'Anthropic Messages',
    description: '适用于 Anthropic Messages API。',
  },
]

export type SettingsTab = 'appearance' | 'llm' | 'embedding' | 'search' | 'notebook' | 'governance' | 'help'
type ConfigTestState = {
  status: 'running' | 'ok' | 'error'
  message: string
}

interface NotebookVault {
  id: string
  name: string
  root: string
  external: boolean
  entries: number
  active: boolean
  available: boolean
}

const settingsTabs: Array<{
  key: SettingsTab
  labelKey:
    | 'settings.tabs.appearance'
    | 'settings.tabs.llm'
    | 'settings.tabs.embedding'
    | 'settings.tabs.search'
    | 'settings.tabs.governance'
    | 'settings.tabs.help'
    | 'settings.tabs.notebook'
  icon: LucideIcon
}> = [
  { key: 'appearance', labelKey: 'settings.tabs.appearance', icon: Palette },
  { key: 'llm', labelKey: 'settings.tabs.llm', icon: Brain },
  { key: 'embedding', labelKey: 'settings.tabs.embedding', icon: Database },
  { key: 'search', labelKey: 'settings.tabs.search', icon: Globe2 },
  { key: 'notebook', labelKey: 'settings.tabs.notebook', icon: BookMarked },
  { key: 'governance', labelKey: 'settings.tabs.governance', icon: SlidersHorizontal },
  { key: 'help', labelKey: 'settings.tabs.help', icon: CircleHelp },
]

export function SettingsPage({
  settings,
  activeTab,
  onTabChange,
  onChange,
  onOpenOnboarding,
  onGuideNavigate,
  onStartGuideAssistant,
}: Props) {
  const { t } = useI18n()
  const [testState, setTestState] = useState<Record<string, ConfigTestState>>({})
  const [dataDir, setDataDir] = useState<string | null>(null)
  const [dataDirError, setDataDirError] = useState('')
  const [notebookVaults, setNotebookVaults] = useState<NotebookVault[]>([])
  const [editingVault, setEditingVault] = useState<{ id: string; name: string } | null>(null)
  const [pendingVaultRemoval, setPendingVaultRemoval] = useState<string | null>(null)
  const [notebookStatus, setNotebookStatus] = useState('笔记设置已就绪')
  const [notebookLoading, setNotebookLoading] = useState(false)

  useEffect(() => {
    let mounted = true
    getDesktopDataDir()
      .then((value) => {
        if (mounted) setDataDir(value)
      })
      .catch((error) => {
        if (mounted) setDataDirError(error instanceof Error ? error.message : 'Failed to load data directory')
      })
    return () => {
      mounted = false
    }
  }, [])

  const refreshNotebookStatus = async () => {
    setNotebookLoading(true)
    try {
      const res = await fetch('/api/notebook/entries?space_id=default')
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookVaults((data.vaults ?? []) as NotebookVault[])
      const entries = Array.isArray(data.entries) ? data.entries.length : 0
      setNotebookStatus(entries ? `已加载 ${entries} 篇笔记` : '还没有笔记')
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  useEffect(() => {
    void refreshNotebookStatus()
  }, [])

  const update = <K extends keyof LlmSettings>(key: K, value: LlmSettings[K]) => {
    onChange({ ...settings, [key]: value })
  }

  const setLanguage = (language: UiLanguage) => {
    onChange({ ...settings, language })
  }

  const bindNotebookVault = async (folderPath: string) => {
    if (!folderPath.trim()) return
    setNotebookLoading(true)
    try {
      const res = await fetch('/api/notebook/vaults', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: folderPath, name: '' }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookVaults((data.vaults ?? []) as NotebookVault[])
      setNotebookStatus(`已添加笔记库：${(data.vault as NotebookVault | undefined)?.name ?? folderPath}`)
      window.dispatchEvent(new Event('folumi:notebook-vaults-changed'))
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const updateNotebookVault = async (vaultId: string, update: { name?: string; active?: boolean }) => {
    setNotebookLoading(true)
    try {
      const res = await fetch(`/api/notebook/vaults/${encodeURIComponent(vaultId)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(update),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookVaults((data.vaults ?? []) as NotebookVault[])
      setEditingVault(null)
      setNotebookStatus(update.active ? '已切换笔记库' : '笔记库名称已更新')
      window.dispatchEvent(new Event('folumi:notebook-vaults-changed'))
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const removeNotebookVault = async (vaultId: string) => {
    setNotebookLoading(true)
    try {
      const res = await fetch(`/api/notebook/vaults/${encodeURIComponent(vaultId)}`, { method: 'DELETE' })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookVaults((data.vaults ?? []) as NotebookVault[])
      setPendingVaultRemoval(null)
      setNotebookStatus('已从 Folumi 移除，磁盘文件未删除')
      window.dispatchEvent(new Event('folumi:notebook-vaults-changed'))
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const chooseNotebookFolder = async () => {
    setNotebookStatus('正在打开文件夹选择器...')
    try {
      const selected = await chooseDesktopDirectory('选择笔记文件夹')
      if (selected) {
        await bindNotebookVault(selected)
      } else {
        setNotebookStatus('已取消选择')
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setNotebookStatus(`无法选择文件夹：${message}`)
    }
  }

  const activeLlmConfig =
    settings.llmConfigs.find((config) => config.id === settings.activeLlmConfigId) ?? null
  const activeEmbeddingConfig =
    settings.embeddingConfigs.find((config) => config.id === settings.activeEmbeddingConfigId) ?? null
  const activeSearchConfig =
    settings.searchConfigs.find((config) => config.id === settings.activeSearchConfigId) ?? null

  const addLlmConfig = () => {
    const config = createLlmConfig()
    onChange({
      ...settings,
      ...legacyFieldsFromLlmConfig(config),
      llmConfigs: [...settings.llmConfigs, config],
      activeLlmConfigId: config.id,
    })
  }

  const updateLlmConfig = <K extends keyof LlmModelConfig>(
    id: string,
    key: K,
    value: LlmModelConfig[K],
  ) => {
    const nextConfigs = settings.llmConfigs.map((config) => {
      if (config.id !== id) return config
      if (key !== 'provider') return { ...config, [key]: value }
      const provider = value as LlmProvider
      const preset = llmProviderPreset(provider)
      return {
        ...config,
        provider,
        name: config.name || preset.label,
        model: preset.model,
        baseUrl: preset.baseUrl,
        chatPath: preset.chatPath,
        contextWindowTokens: preset.contextWindowTokens,
      }
    })
    const active = nextConfigs.find((config) => config.id === settings.activeLlmConfigId)
    onChange({
      ...settings,
      ...(active ? legacyFieldsFromLlmConfig(active) : {}),
      llmConfigs: nextConfigs,
    })
  }

  const selectLlmConfig = (id: string) => {
    const config = settings.llmConfigs.find((item) => item.id === id)
    onChange({
      ...settings,
      ...(config ? legacyFieldsFromLlmConfig(config) : {}),
      activeLlmConfigId: id,
    })
  }

  const deleteLlmConfig = (id: string) => {
    const nextConfigs = settings.llmConfigs.filter((config) => config.id !== id)
    const nextActiveId =
      settings.activeLlmConfigId === id ? nextConfigs[0]?.id ?? null : settings.activeLlmConfigId
    const active = nextConfigs.find((config) => config.id === nextActiveId)
    onChange({
      ...settings,
      ...(active ? legacyFieldsFromLlmConfig(active) : {}),
      llmConfigs: nextConfigs,
      activeLlmConfigId: nextActiveId,
    })
  }

  const addEmbeddingConfig = () => {
    const config = createEmbeddingConfig()
    onChange({
      ...settings,
      embeddingConfigs: [...settings.embeddingConfigs, config],
      activeEmbeddingConfigId: config.id,
    })
  }

  const updateEmbeddingConfig = <K extends keyof EmbeddingModelConfig>(
    id: string,
    key: K,
    value: EmbeddingModelConfig[K],
  ) => {
    onChange({
      ...settings,
      embeddingConfigs: settings.embeddingConfigs.map((config) =>
        config.id === id ? { ...config, [key]: value } : config,
      ),
    })
  }

  const deleteEmbeddingConfig = (id: string) => {
    const nextConfigs = settings.embeddingConfigs.filter((config) => config.id !== id)
    onChange({
      ...settings,
      embeddingConfigs: nextConfigs,
      activeEmbeddingConfigId:
        settings.activeEmbeddingConfigId === id
          ? nextConfigs[0]?.id ?? null
          : settings.activeEmbeddingConfigId,
    })
  }

  const addSearchConfig = () => {
    const config = createSearchConfig()
    onChange({
      ...settings,
      searchConfigs: [...settings.searchConfigs, config],
      activeSearchConfigId: config.id,
    })
  }

  const updateSearchConfig = <K extends keyof SearchConfig>(
    id: string,
    key: K,
    value: SearchConfig[K],
  ) => {
    onChange({
      ...settings,
      searchConfigs: settings.searchConfigs.map((config) =>
        config.id === id ? { ...config, [key]: value } : config,
      ),
    })
  }

  const updateSearchProvider = (id: string, provider: SearchProvider) => {
    const preset = searchProviderPreset(provider)
    onChange({
      ...settings,
      searchConfigs: settings.searchConfigs.map((config) =>
        config.id === id
          ? {
              ...config,
              provider,
              name: config.name === 'DuckDuckGo' || config.name === 'Bing' ? preset.name : config.name,
              baseUrl: preset.baseUrl,
            }
          : config,
      ),
    })
  }

  const deleteSearchConfig = (id: string) => {
    const nextConfigs = settings.searchConfigs.filter((config) => config.id !== id)
    onChange({
      ...settings,
      searchConfigs: nextConfigs,
      activeSearchConfigId:
        settings.activeSearchConfigId === id
          ? nextConfigs[0]?.id ?? null
          : settings.activeSearchConfigId,
    })
  }

  const setConfigTestState = (id: string, state: ConfigTestState) => {
    setTestState((current) => ({ ...current, [id]: state }))
  }

  const testLlmConfig = async (config: LlmModelConfig) => {
    setConfigTestState(config.id, { status: 'running', message: 'Testing model connection...' })
    try {
      const payload = await testLlmConnection(config)
      const confirmedWindow = Number(payload.confirmed_context_window_tokens || 0)
      if (confirmedWindow > 0 && confirmedWindow !== config.contextWindowTokens) {
        updateLlmConfig(config.id, 'contextWindowTokens', confirmedWindow)
      }
      const usage =
        payload.input_tokens || payload.output_tokens
          ? ` Input ${payload.input_tokens ?? 0}, output ${payload.output_tokens ?? 0} tokens.`
          : ''
      setConfigTestState(config.id, {
        status: 'ok',
        message: `${payload.message || 'Model connection works.'}${usage}`,
      })
    } catch (error) {
      setConfigTestState(config.id, {
        status: 'error',
        message: error instanceof Error ? error.message : 'Model test failed',
      })
    }
  }

  const testEmbeddingConfig = async (config: EmbeddingModelConfig) => {
    setConfigTestState(config.id, { status: 'running', message: 'Testing embedding connection...' })
    try {
      const response = await fetch('/api/settings/test/embedding', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          provider: config.provider,
          model: config.model,
          api_key: config.apiKey,
          base_url: config.baseUrl,
          embeddings_path: config.embeddingsPath,
          dimensions: config.dimensions,
          send_dimensions: config.sendDimensions,
        }),
      })
      const payload = await response.json().catch(() => ({}))
      if (!response.ok) {
        throw new Error(payload.error || 'Embedding test failed')
      }
      const dimensions = Number(payload.dimensions || 0)
      if (dimensions > 0 && dimensions !== config.dimensions) {
        updateEmbeddingConfig(config.id, 'dimensions', dimensions)
      }
      const usage = payload.total_tokens ? ` Total ${payload.total_tokens} tokens.` : ''
      setConfigTestState(config.id, {
        status: 'ok',
        message: `${payload.message || 'Embedding connection works.'}${usage}`,
      })
    } catch (error) {
      setConfigTestState(config.id, {
        status: 'error',
        message: error instanceof Error ? error.message : 'Embedding test failed',
      })
    }
  }

  const handleOpenDataDir = async () => {
    setDataDirError('')
    try {
      await openDesktopDataDir()
    } catch (error) {
      setDataDirError(error instanceof Error ? error.message : 'Failed to open data directory')
    }
  }

  return (
    <main className="flex min-h-0 flex-1 bg-gray-50">
      <aside className="hidden w-64 shrink-0 border-r border-gray-200 bg-white px-4 py-6 md:block">
        <div className="mb-8 px-2">
          <h2 className="text-xl font-semibold text-gray-900">{t('settings.title')}</h2>
          <p className="mt-2 text-sm leading-6 text-gray-600">{t('settings.subtitle')}</p>
        </div>

        <nav className="space-y-1">
          {settingsTabs.map((tab) => {
            const Icon = tab.icon
            const active = activeTab === tab.key
            return (
              <button
                key={tab.key}
                type="button"
                className={`flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm ${
                  active ? 'bg-gray-900 text-white' : 'text-gray-700 hover:bg-gray-100 hover:text-gray-900'
                }`}
                onClick={() => onTabChange(tab.key)}
              >
                <Icon size={18} />
                <span>{t(tab.labelKey)}</span>
              </button>
            )
          })}
        </nav>
      </aside>

      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl px-5 py-6 md:px-8">
          <div className="mb-6 flex flex-wrap items-center gap-3">
            <div className="md:hidden">
              <label className="sr-only" htmlFor="settings-tab">
                设置分类
              </label>
              <select
                id="settings-tab"
                className="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
                value={activeTab}
                onChange={(event) => onTabChange(event.target.value as SettingsTab)}
              >
                {settingsTabs.map((tab) => (
                  <option key={tab.key} value={tab.key}>
                    {t(tab.labelKey)}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <h2 className="text-xl font-semibold text-gray-900">
                {t(settingsTabs.find((tab) => tab.key === activeTab)?.labelKey ?? 'settings.tabs.llm')}
              </h2>
              <p className="mt-1 text-sm text-gray-600">{tabDescription(activeTab, t)}</p>
            </div>
            {activeTab !== 'help' && (
              <span className="ml-auto text-sm text-gray-500">{t('settings.saved')}</span>
            )}
          </div>

          {activeTab === 'appearance' && (
            <SettingsPanel
              icon={Palette}
              title={t('settings.appearance.title')}
              description={t('settings.appearance.description')}
            >
              <div className="space-y-3 rounded-lg border border-gray-200 px-4 py-4">
                <div>
                  <div className="text-sm font-medium text-gray-900">{t('settings.theme.title')}</div>
                  <div className="mt-1 text-sm text-gray-500">{t('settings.theme.description')}</div>
                </div>
                <div className="grid gap-3 sm:grid-cols-2">
                  <ThemeOption
                    theme="cool-light"
                    selected={settings.theme === 'cool-light'}
                    icon={Sun}
                    title={t('settings.theme.coolLight')}
                    description={t('settings.theme.coolLight.description')}
                    colors={['#eceff3', '#f8f9fa', '#ffffff', '#e1e5ea', '#2563eb']}
                    onSelect={(theme) => update('theme', theme)}
                  />
                  <ThemeOption
                    theme="graphite-dark"
                    selected={settings.theme === 'graphite-dark'}
                    icon={Moon}
                    title={t('settings.theme.graphiteDark')}
                    description={t('settings.theme.graphiteDark.description')}
                    colors={['#202328', '#151719', '#24272c', '#34383f', '#60a5fa']}
                    onSelect={(theme) => update('theme', theme)}
                  />
                </div>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-gray-200 px-4 py-3">
                <div>
                  <div className="text-sm font-medium text-gray-900">{t('settings.language.title')}</div>
                  <div className="mt-1 text-sm text-gray-500">
                    {settings.language === 'en-US'
                      ? t('settings.language.description.en')
                      : t('settings.language.description.zh')}
                  </div>
                </div>
                <div className="inline-flex rounded-md border border-gray-300 bg-gray-50 p-1 text-sm">
                  <button
                    type="button"
                    className={`px-3 py-1 ${
                      settings.language === 'en-US'
                        ? 'rounded bg-white text-gray-900 shadow-sm'
                        : 'text-gray-500'
                    }`}
                    onClick={() => setLanguage('en-US')}
                  >
                    {t('settings.language.english')}
                  </button>
                  <button
                    type="button"
                    className={`px-3 py-1 ${
                      settings.language === 'zh-CN'
                        ? 'rounded bg-white text-gray-900 shadow-sm'
                        : 'text-gray-500'
                    }`}
                    onClick={() => setLanguage('zh-CN')}
                  >
                    {t('settings.language.chinese')}
                  </button>
                </div>
              </div>
            </SettingsPanel>
          )}

          {activeTab === 'llm' && (
            <SettingsPanel icon={Brain} title="LLM" description="选择用来回答问题的模型。">
              {settings.llmConfigs.length === 0 ? (
                <EmptyConfig onAdd={addLlmConfig} label="还没有 LLM 配置" />
              ) : (
                <div className="grid gap-5 lg:grid-cols-[230px_1fr]">
                  <ConfigList
                    items={settings.llmConfigs.map((config) => ({
                      id: config.id,
                      title: config.name || llmProviderPreset(config.provider).label,
                      subtitle: config.model || '未设置模型',
                    }))}
                    activeId={settings.activeLlmConfigId}
                    addLabel="添加配置"
                    onAdd={addLlmConfig}
                    onSelect={selectLlmConfig}
                  />

                  {activeLlmConfig && (
                    <div className="space-y-5 rounded-lg border border-gray-200 p-4">
                      <ConfigHeader
                        title="模型接口"
                        description="填写服务商提供的连接信息。"
                        onDelete={() => deleteLlmConfig(activeLlmConfig.id)}
                      />
                      <ConfigTestBar
                        state={testState[activeLlmConfig.id]}
                        label="测试配置"
                        onTest={() => testLlmConfig(activeLlmConfig)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label="配置名称">
                          <TextInput
                            value={activeLlmConfig.name}
                            onChange={(value) => updateLlmConfig(activeLlmConfig.id, 'name', value)}
                          />
                        </Field>

                        <Field label="接口模式">
                          <select
                            className={inputClassName}
                            value={activeLlmConfig.provider}
                            onChange={(event) =>
                              updateLlmConfig(activeLlmConfig.id, 'provider', event.target.value as LlmProvider)
                            }
                          >
                            {providerOptions.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <p className="mt-1 text-xs text-gray-500">
                            {providerOptions.find((option) => option.value === activeLlmConfig.provider)?.description}
                          </p>
                        </Field>

                        <Field label="模型 ID">
                          <TextInput
                            value={activeLlmConfig.model}
                            onChange={(value) => updateLlmConfig(activeLlmConfig.id, 'model', value)}
                          />
                        </Field>

                        <Field label="API Key">
                          <TextInput
                            type="password"
                            value={activeLlmConfig.apiKey}
                            placeholder="sk-..."
                            onChange={(value) => updateLlmConfig(activeLlmConfig.id, 'apiKey', value)}
                          />
                        </Field>

                        <Field label="Base URL">
                          <TextInput
                            value={activeLlmConfig.baseUrl}
                            onChange={(value) => updateLlmConfig(activeLlmConfig.id, 'baseUrl', value)}
                          />
                        </Field>

                        <Field label="Chat path">
                          <TextInput
                            value={activeLlmConfig.chatPath}
                            placeholder="/v1/chat/completions"
                            onChange={(value) => updateLlmConfig(activeLlmConfig.id, 'chatPath', value)}
                          />
                        </Field>

                        <Field label="上下文长度（tokens）">
                          <TextInput
                            type="number"
                            min="1024"
                            step="1024"
                            value={String(activeLlmConfig.contextWindowTokens)}
                            onChange={(value) =>
                              updateLlmConfig(activeLlmConfig.id, 'contextWindowTokens', Number(value))
                            }
                          />
                        </Field>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </SettingsPanel>
          )}

          {activeTab === 'embedding' && (
            <SettingsPanel icon={Database} title="嵌入模型" description="用于从资料中找到相关内容。">
              {settings.embeddingConfigs.length === 0 ? (
                <EmptyConfig onAdd={addEmbeddingConfig} label="还没有嵌入模型配置" />
              ) : (
                <div className="grid gap-5 lg:grid-cols-[230px_1fr]">
                  <ConfigList
                    items={settings.embeddingConfigs.map((config) => ({
                      id: config.id,
                      title: config.name || 'OpenAI',
                      subtitle: config.model || '未设置模型',
                    }))}
                    activeId={settings.activeEmbeddingConfigId}
                    addLabel="添加配置"
                    onAdd={addEmbeddingConfig}
                    onSelect={(id) => update('activeEmbeddingConfigId', id)}
                  />

                  {activeEmbeddingConfig && (
                    <div className="space-y-5 rounded-lg border border-gray-200 p-4">
                      <ConfigHeader
                        title="嵌入模型接口"
                        description="填写模型服务商提供的连接信息。"
                        onDelete={() => deleteEmbeddingConfig(activeEmbeddingConfig.id)}
                      />
                      <ConfigTestBar
                        state={testState[activeEmbeddingConfig.id]}
                        label="测试配置"
                        onTest={() => testEmbeddingConfig(activeEmbeddingConfig)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label="配置名称">
                          <TextInput
                            value={activeEmbeddingConfig.name}
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'name', value)
                            }
                          />
                        </Field>

                        <Field label="接口模式">
                          <select
                            className={inputClassName}
                            value={activeEmbeddingConfig.provider}
                            onChange={() => updateEmbeddingConfig(activeEmbeddingConfig.id, 'provider', 'openai')}
                          >
                            <option value="openai">OpenAI-compatible</option>
                          </select>
                        </Field>

                        <Field label="Base URL">
                          <TextInput
                            value={activeEmbeddingConfig.baseUrl}
                            placeholder="https://api.openai.com"
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'baseUrl', value)
                            }
                          />
                        </Field>

                        <Field label="Embeddings path">
                          <TextInput
                            value={activeEmbeddingConfig.embeddingsPath}
                            placeholder="/v1/embeddings"
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'embeddingsPath', value)
                            }
                          />
                        </Field>

                        <Field label="API Key">
                          <TextInput
                            type="password"
                            value={activeEmbeddingConfig.apiKey}
                            placeholder="sk-..."
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'apiKey', value)
                            }
                          />
                        </Field>

                        <Field label="模型 ID">
                          <TextInput
                            value={activeEmbeddingConfig.model}
                            placeholder="text-embedding-3-small"
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'model', value)
                            }
                          />
                        </Field>

                        <Field label="维度">
                          <TextInput
                            type="number"
                            min="1"
                            value={String(activeEmbeddingConfig.dimensions)}
                            onChange={(value) =>
                              updateEmbeddingConfig(activeEmbeddingConfig.id, 'dimensions', Number(value))
                            }
                          />
                        </Field>

                        <label className="flex items-center gap-3 self-end py-2 text-sm text-gray-800">
                          <input
                            className="h-4 w-4"
                            type="checkbox"
                            checked={activeEmbeddingConfig.sendDimensions}
                            onChange={(event) =>
                              updateEmbeddingConfig(
                                activeEmbeddingConfig.id,
                                'sendDimensions',
                                event.target.checked,
                              )
                            }
                          />
                          发送 dimensions 参数
                        </label>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </SettingsPanel>
          )}

          {activeTab === 'search' && (
            <SettingsPanel icon={Globe2} title="联网搜索" description="让助手需要时查找网页。">
              {settings.searchConfigs.length === 0 ? (
                <EmptyConfig onAdd={addSearchConfig} label="还没有搜索服务" />
              ) : (
                <div className="grid gap-5 lg:grid-cols-[230px_1fr]">
                  <ConfigList
                    items={settings.searchConfigs.map((config) => ({
                      id: config.id,
                      title: config.name || 'DuckDuckGo',
                      subtitle: `${config.provider} · 最多 ${config.maxResults} 条`,
                    }))}
                    activeId={settings.activeSearchConfigId}
                    addLabel="添加配置"
                    onAdd={addSearchConfig}
                    onSelect={(id) => update('activeSearchConfigId', id)}
                  />

                  {activeSearchConfig && (
                    <div className="space-y-5 rounded-lg border border-gray-200 p-4">
                      <ConfigHeader
                        title="搜索服务"
                        description="选择免费服务，或填写付费服务的 API。"
                        onDelete={() => deleteSearchConfig(activeSearchConfig.id)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label="配置名称">
                          <TextInput
                            value={activeSearchConfig.name}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'name', value)
                            }
                          />
                        </Field>

                        <Field label="服务商">
                          <select
                            className={inputClassName}
                            value={activeSearchConfig.provider}
                            onChange={(event) =>
                              updateSearchProvider(
                                activeSearchConfig.id,
                                event.target.value as SearchProvider,
                              )
                            }
                          >
                            <option value="duckduckgo">DuckDuckGo</option>
                            <option value="bing">Bing</option>
                            <option value="brave">Brave Search API</option>
                            <option value="tavily">Tavily</option>
                            <option value="serper">Serper</option>
                            <option value="serpapi">SerpAPI</option>
                            <option value="exa">Exa</option>
                          </select>
                        </Field>

                        <Field label="Base URL">
                          <TextInput
                            value={activeSearchConfig.baseUrl}
                            placeholder={searchProviderPreset(activeSearchConfig.provider).baseUrl}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'baseUrl', value)
                            }
                          />
                        </Field>

                        <Field label="API Key">
                          <TextInput
                            type="password"
                            value={activeSearchConfig.apiKey}
                            placeholder={
                              activeSearchConfig.provider === 'duckduckgo' ||
                              activeSearchConfig.provider === 'bing'
                                ? '可选'
                                : '必填'
                            }
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'apiKey', value)
                            }
                          />
                        </Field>

                        <Field label="最多结果数">
                          <TextInput
                            type="number"
                            min="1"
                            max="10"
                            value={String(activeSearchConfig.maxResults)}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'maxResults', Number(value))
                            }
                          />
                        </Field>

                        <Field label="网页读取超时（秒）">
                          <TextInput
                            type="number"
                            min="3"
                            max="60"
                            value={String(activeSearchConfig.fetchTimeoutSecs)}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'fetchTimeoutSecs', Number(value))
                            }
                          />
                        </Field>

                        <Field label="网页最大读取字数">
                          <TextInput
                            type="number"
                            min="1000"
                            max="60000"
                            step="1000"
                            value={String(activeSearchConfig.maxFetchChars)}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'maxFetchChars', Number(value))
                            }
                          />
                        </Field>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </SettingsPanel>
          )}

          {activeTab === 'notebook' && (
            <SettingsPanel icon={BookMarked} title="笔记存储" description="管理笔记库使用的文件夹。">
              <div className="space-y-3 rounded-lg border border-gray-200 bg-white p-4">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <div className="text-sm font-medium text-gray-900">笔记库</div>
                    <div className="mt-1 text-xs text-gray-500">每个笔记库对应一个独立文件夹。</div>
                  </div>
                  <button
                    type="button"
                    className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={notebookLoading}
                    onClick={() => void chooseNotebookFolder()}
                  >
                    <Plus size={15} />
                    添加笔记库
                  </button>
                </div>

                <div className="space-y-2">
                  {notebookVaults.length === 0 && (
                    <div className="rounded-xl border border-dashed border-sky-200 bg-sky-50/40 px-4 py-5 text-center">
                      <div className="text-sm font-medium text-slate-700">还没有笔记库</div>
                      <div className="mt-1 text-xs text-slate-500">添加一个文件夹后即可开始使用笔记。</div>
                    </div>
                  )}
                  {notebookVaults.map((vault) => (
                    <div key={vault.id} className={`rounded-lg border p-3 ${vault.active ? 'border-blue-200 bg-blue-50/50' : 'border-gray-200'}`}>
                      <div className="flex items-start gap-3">
                        <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-md ${vault.active ? 'bg-white text-blue-600' : 'bg-gray-50 text-gray-500'}`}>
                          <FolderOpen size={18} />
                        </span>
                        <div className="min-w-0 flex-1">
                          {editingVault?.id === vault.id ? (
                            <div className="flex gap-2">
                              <input
                                className="h-8 min-w-0 flex-1 rounded-md border border-blue-300 bg-white px-2 text-sm outline-none focus:ring-2 focus:ring-blue-100"
                                value={editingVault.name}
                                maxLength={60}
                                autoFocus
                                onChange={(event) => setEditingVault({ id: vault.id, name: event.target.value })}
                                onKeyDown={(event) => {
                                  if (event.key === 'Enter') void updateNotebookVault(vault.id, { name: editingVault.name })
                                  if (event.key === 'Escape') setEditingVault(null)
                                }}
                              />
                              <button type="button" className="rounded-md bg-blue-600 px-2 text-xs text-white" onClick={() => void updateNotebookVault(vault.id, { name: editingVault.name })}>保存</button>
                            </div>
                          ) : (
                            <div className="flex items-center gap-2 text-sm font-medium text-gray-900">
                              <span className="truncate">{vault.name}</span>
                              {vault.active && <span className="rounded-full bg-blue-100 px-2 py-0.5 text-[11px] text-blue-700">当前</span>}
                              {!vault.available && <span className="rounded-full bg-red-50 px-2 py-0.5 text-[11px] text-red-600">无法访问</span>}
                            </div>
                          )}
                          <div className="mt-1 truncate font-mono text-xs text-gray-400" title={vault.root}>{vault.root}</div>
                          <div className="mt-2 text-xs text-gray-500">{vault.entries} 篇笔记 · {vault.external ? '外部文件夹' : '应用内存储'}</div>
                        </div>
                        <div className="flex shrink-0 items-center gap-1">
                          {!vault.active && vault.available && (
                            <button type="button" className="rounded-md px-2 py-1 text-xs text-gray-600 hover:bg-white" disabled={notebookLoading} onClick={() => void updateNotebookVault(vault.id, { active: true })}>切换</button>
                          )}
                          <button type="button" className="rounded-md px-2 py-1 text-xs text-gray-600 hover:bg-white" disabled={notebookLoading} onClick={() => setEditingVault({ id: vault.id, name: vault.name })}>重命名</button>
                          <button type="button" className="rounded-md px-2 py-1 text-xs text-red-600 hover:bg-red-50 disabled:opacity-40" disabled={notebookLoading} onClick={() => setPendingVaultRemoval(vault.id)}>移除</button>
                        </div>
                      </div>
                      {pendingVaultRemoval === vault.id && (
                        <div className="mt-3 flex items-center justify-between gap-3 rounded-md bg-red-50 px-3 py-2 text-xs text-red-700">
                          <span>只解除关联，不会删除文件夹中的任何文件。</span>
                          <span className="flex shrink-0 gap-2">
                            <button type="button" className="rounded px-2 py-1 hover:bg-white" onClick={() => setPendingVaultRemoval(null)}>取消</button>
                            <button type="button" className="rounded bg-red-600 px-2 py-1 text-white" onClick={() => void removeNotebookVault(vault.id)}>确认移除</button>
                          </span>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              </div>

              <div className={`rounded-md border px-3 py-2 text-sm ${
                notebookStatus.toLowerCase().includes('failed') || notebookStatus.toLowerCase().includes('error')
                  ? 'border-red-200 bg-red-50 text-red-700'
                  : 'border-gray-200 bg-gray-50 text-gray-600'
              }`}>
                {notebookLoading ? '处理中...' : notebookStatus}
              </div>
            </SettingsPanel>
          )}

          {activeTab === 'governance' && (
            <SettingsPanel icon={Activity} title="权限与数据" description="管理操作确认和本地数据。">
              <label className="flex items-center gap-3 text-sm text-gray-800">
                <input
                  className="h-4 w-4"
                  type="checkbox"
                  checked={settings.requireApproval}
                  onChange={(event) => update('requireApproval', event.target.checked)}
                />
                执行操作前先询问我
              </label>

              <div className="rounded-lg border border-gray-200 bg-white px-4 py-4">
                <div className="flex flex-wrap items-start gap-3">
                  <div className="flex h-9 w-9 items-center justify-center rounded-md bg-blue-50 text-blue-600">
                    <FolderOpen size={18} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-gray-900">本地数据目录</div>
                    <div className="mt-1 break-all font-mono text-xs text-gray-500">
                      {dataDir ?? '仅桌面应用可用；浏览器开发模式使用仓库内 .llm-tutor。'}
                    </div>
                    {dataDirError && <div className="mt-2 text-xs text-red-600">{dataDirError}</div>}
                  </div>
                  <button
                    type="button"
                    className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={!dataDir}
                    onClick={handleOpenDataDir}
                  >
                    <FolderOpen size={15} />
                    打开
                  </button>
                </div>
              </div>
            </SettingsPanel>
          )}

          {activeTab === 'help' && (
            <ProductGuide
              onNavigate={onGuideNavigate}
              onStartGuideAssistant={onStartGuideAssistant}
              onRestartOnboarding={onOpenOnboarding}
            />
          )}
        </div>
      </div>
    </main>
  )
}

function ThemeOption({
  theme,
  selected,
  icon: Icon,
  title,
  description,
  colors,
  onSelect,
}: {
  theme: ThemeId
  selected: boolean
  icon: LucideIcon
  title: string
  description: string
  colors: string[]
  onSelect: (theme: ThemeId) => void
}) {
  return (
    <button
      type="button"
      className={`relative flex min-h-36 flex-col rounded-lg border p-4 text-left transition ${
        selected
          ? 'border-blue-500 bg-blue-50 ring-2 ring-blue-100'
          : 'border-gray-200 bg-white hover:border-gray-300 hover:bg-gray-50'
      }`}
      aria-pressed={selected}
      onClick={() => onSelect(theme)}
    >
      <span className="flex w-full items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-gray-200 bg-white text-gray-700">
          <Icon size={18} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-semibold text-gray-900">{title}</span>
          <span className="mt-1 block text-xs leading-5 text-gray-500">{description}</span>
        </span>
        {selected && (
          <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-blue-600 text-white">
            <Check size={13} />
          </span>
        )}
      </span>
      <span className="mt-auto flex w-full overflow-hidden rounded-md border border-gray-200" aria-hidden="true">
        {colors.map((color) => (
          <span key={color} className="h-7 flex-1" style={{ backgroundColor: color }} />
        ))}
      </span>
    </button>
  )
}

function tabDescription(tab: SettingsTab, t: (key: TranslationKey) => string) {
  const keyByTab: Record<SettingsTab, TranslationKey> = {
    appearance: 'settings.appearance.description',
    llm: 'settings.llm.description',
    embedding: 'settings.embedding.description',
    search: 'settings.search.description',
    notebook: 'settings.notebook.description',
    governance: 'settings.governance.description',
    help: 'settings.help.description',
  }
  return t(keyByTab[tab])
}

async function safeJson(response: Response): Promise<Record<string, unknown>> {
  try {
    return await response.json() as Record<string, unknown>
  } catch {
    return {}
  }
}

function errorMessage(data: Record<string, unknown>, status: number) {
  const error = data.error
  if (typeof error === 'string' && error.trim()) return error
  const message = data.message
  if (typeof message === 'string' && message.trim()) return message
  return `HTTP ${status}`
}

function EmptyConfig({ label, onAdd }: { label: string; onAdd: () => void }) {
  return (
    <div className="flex min-h-40 flex-col items-center justify-center rounded-lg border border-dashed border-gray-300 bg-gray-50 px-4 py-8 text-center">
      <p className="text-sm text-gray-500">{label}</p>
      <button
        type="button"
        className="mt-4 inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50"
        onClick={onAdd}
      >
        <Plus size={16} />
        添加配置
      </button>
    </div>
  )
}

function ConfigList({
  items,
  activeId,
  addLabel,
  onAdd,
  onSelect,
}: {
  items: Array<{ id: string; title: string; subtitle: string }>
  activeId: string | null
  addLabel: string
  onAdd: () => void
  onSelect: (id: string) => void
}) {
  return (
    <div className="space-y-2">
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={`w-full rounded-lg border px-4 py-3 text-left ${
            item.id === activeId
              ? 'border-gray-900 bg-gray-50'
              : 'border-gray-200 bg-white hover:bg-gray-50'
          }`}
          onClick={() => onSelect(item.id)}
        >
          <div className="text-sm font-semibold text-gray-900">{item.title}</div>
          <div className="mt-1 truncate text-xs text-gray-500">{item.subtitle}</div>
        </button>
      ))}
      <button
        type="button"
        className="inline-flex w-full items-center justify-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50"
        onClick={onAdd}
      >
        <Plus size={16} />
        {addLabel}
      </button>
    </div>
  )
}

function ConfigHeader({
  title,
  description = '配置会保存到本地浏览器，并用于新建会话。',
  onDelete,
}: {
  title: string
  description?: string
  onDelete: () => void
}) {
  return (
    <div className="flex items-center gap-3">
      <div>
        <h4 className="text-sm font-semibold text-gray-900">{title}</h4>
        <p className="mt-1 text-xs text-gray-500">{description}</p>
      </div>
      <button
        type="button"
        className="ml-auto inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-gray-500 hover:bg-gray-50 hover:text-gray-900"
        onClick={onDelete}
        title="删除配置"
        aria-label="删除配置"
      >
        <Trash2 size={15} />
      </button>
    </div>
  )
}

function ConfigTestBar({
  state,
  label,
  onTest,
}: {
  state?: ConfigTestState
  label: string
  onTest: () => void
}) {
  const running = state?.status === 'running'
  const tone =
    state?.status === 'ok'
      ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
      : state?.status === 'error'
        ? 'border-red-200 bg-red-50 text-red-700'
        : 'border-gray-200 bg-gray-50 text-gray-600'
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-md border border-gray-200 bg-gray-50 p-3">
      <button
        type="button"
        className="inline-flex items-center justify-center rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-medium text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={running}
        onClick={onTest}
      >
        {running ? '测试中...' : label}
      </button>
      {state && (
        <div className={`min-w-0 flex-1 rounded-md border px-3 py-2 text-sm ${tone}`}>
          {state.message}
        </div>
      )}
    </div>
  )
}

function SettingsPanel({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <section className="rounded-lg border border-gray-200 bg-white p-5">
      <div className="mb-5 flex items-start gap-3">
        <div className="flex h-9 w-9 items-center justify-center rounded-md border border-gray-200 bg-gray-50 text-gray-700">
          <Icon size={18} />
        </div>
        <div>
          <h3 className="text-sm font-semibold text-gray-900">{title}</h3>
          <p className="mt-1 text-sm text-gray-500">{description}</p>
        </div>
      </div>
      <div className="space-y-5">{children}</div>
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-sm font-medium text-gray-700">{label}</span>
      {children}
    </label>
  )
}

function TextInput({
  className = '',
  onChange,
  ...props
}: Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange'> & {
  onChange: (value: string) => void
}) {
  return (
    <input
      {...props}
      className={`${inputClassName} ${className}`}
      onChange={(event) => onChange(event.target.value)}
    />
  )
}

function legacyFieldsFromLlmConfig(config: LlmModelConfig) {
  return {
    provider: config.provider,
    model: config.model,
    apiKey: config.apiKey,
    baseUrl: config.baseUrl,
    chatPath: config.chatPath,
  }
}

const inputClassName =
  'w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm outline-none focus:border-gray-900'
