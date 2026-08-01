import { useEffect, useRef, useState, type ReactNode } from 'react'
import {
  Activity,
  BookMarked,
  Brain,
  Check,
  CircleHelp,
  Database,
  Download,
  FolderOpen,
  Globe2,
  Palette,
  Plus,
  Moon,
  SlidersHorizontal,
  Sun,
  Trash2,
  Upload,
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
import type { SourceReference, SourceTarget } from './MarkdownMessage'
import { UserMemoryPage } from './UserMemoryPage'
import { ProductGuide } from './ProductGuide'

interface Props {
  settings: LlmSettings
  activeTab: SettingsTab
  onTabChange: (tab: SettingsTab) => void
  onChange: (settings: LlmSettings) => void
  onOpenOnboarding: () => void
  onGuideNavigate: (destination: ProductGuideDestination) => void
  onStartGuideTutor: () => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
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

export type SettingsTab = 'assistant' | 'appearance' | 'llm' | 'embedding' | 'search' | 'notebook' | 'governance' | 'help'
type ConfigTestState = {
  status: 'running' | 'ok' | 'error'
  message: string
}

interface NotebookImportPreviewItem {
  source_path: string
  title: string
  markdown_chars: number
  tags: string[]
  duplicate_title_entry_id?: string | null
  duplicate_title?: string | null
}

interface NotebookImportSkipped {
  file_name: string
  reason: string
}

interface NotebookImportPreview {
  items: NotebookImportPreviewItem[]
  skipped: NotebookImportSkipped[]
  conflict_count: number
}

interface NotebookImportResult {
  imported_count: number
  skipped: NotebookImportSkipped[]
}

interface NotebookVault {
  root: string
  external: boolean
  entries: number
}

const settingsTabs: Array<{
  key: SettingsTab
  labelKey:
    | 'settings.tabs.appearance'
    | 'settings.tabs.assistant'
    | 'settings.tabs.llm'
    | 'settings.tabs.embedding'
    | 'settings.tabs.search'
    | 'settings.tabs.governance'
    | 'settings.tabs.help'
    | 'space.tabs.notebook'
  icon: LucideIcon
}> = [
  { key: 'assistant', labelKey: 'settings.tabs.assistant', icon: Brain },
  { key: 'appearance', labelKey: 'settings.tabs.appearance', icon: Palette },
  { key: 'llm', labelKey: 'settings.tabs.llm', icon: Brain },
  { key: 'embedding', labelKey: 'settings.tabs.embedding', icon: Database },
  { key: 'search', labelKey: 'settings.tabs.search', icon: Globe2 },
  { key: 'notebook', labelKey: 'space.tabs.notebook', icon: BookMarked },
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
  onStartGuideTutor,
  onSourceNavigate,
}: Props) {
  const { t } = useI18n()
  const [testState, setTestState] = useState<Record<string, ConfigTestState>>({})
  const [dataDir, setDataDir] = useState<string | null>(null)
  const [dataDirError, setDataDirError] = useState('')
  const [notebookVault, setNotebookVault] = useState<NotebookVault | null>(null)
  const [notebookStatus, setNotebookStatus] = useState('Notebook settings ready')
  const [notebookLoading, setNotebookLoading] = useState(false)
  const [pendingImportFiles, setPendingImportFiles] = useState<File[]>([])
  const [importPreview, setImportPreview] = useState<NotebookImportPreview | null>(null)
  const [importResult, setImportResult] = useState<NotebookImportResult | null>(null)
  const importInputRef = useRef<HTMLInputElement | null>(null)

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
      setNotebookVault((data.vault ?? null) as NotebookVault | null)
      const entries = Array.isArray(data.entries) ? data.entries.length : 0
      setNotebookStatus(entries ? `${entries} notes loaded` : 'No notebook notes yet')
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

  const previewNotebookFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return
    const nextFiles = Array.from(files)
    const form = new FormData()
    form.append('space_id', 'default')
    nextFiles.forEach((file) => form.append('file', file))
    setPendingImportFiles(nextFiles)
    setImportPreview(null)
    setImportResult(null)
    setNotebookLoading(true)
    try {
      const res = await fetch('/api/notebook/import/preview', {
        method: 'POST',
        body: form,
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const preview = data as unknown as NotebookImportPreview
      setImportPreview(preview)
      setNotebookStatus(`Previewed ${preview.items.length} note${preview.items.length === 1 ? '' : 's'}`)
    } catch (err) {
      setPendingImportFiles([])
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const importNotebookFiles = async () => {
    if (pendingImportFiles.length === 0) return
    const form = new FormData()
    form.append('space_id', 'default')
    pendingImportFiles.forEach((file) => form.append('file', file))
    setNotebookLoading(true)
    try {
      const res = await fetch('/api/notebook/import', {
        method: 'POST',
        body: form,
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      const imported = Array.isArray(data.entries) ? data.entries.length : 0
      const skippedItems = Array.isArray(data.skipped) ? data.skipped as NotebookImportSkipped[] : []
      setImportResult({ imported_count: imported, skipped: skippedItems })
      setPendingImportFiles([])
      setImportPreview(null)
      setNotebookStatus(`Imported ${imported} note${imported === 1 ? '' : 's'}${skippedItems.length ? `, skipped ${skippedItems.length}` : ''}`)
      await refreshNotebookStatus()
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const bindNotebookVault = async (folderPath: string) => {
    if (!folderPath.trim()) return
    setPendingImportFiles([])
    setImportPreview(null)
    setImportResult(null)
    setNotebookLoading(true)
    try {
      const res = await fetch('/api/notebook/vault', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: folderPath }),
      })
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      setNotebookVault((data.vault ?? null) as NotebookVault | null)
      setNotebookStatus(`Bound notebook vault: ${(data.vault as NotebookVault | undefined)?.root ?? folderPath}`)
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
    }
  }

  const chooseNotebookFolder = async () => {
    setNotebookStatus('Opening folder picker...')
    try {
      const selected = await chooseDesktopDirectory('Bind Notebook Vault folder')
      if (selected) {
        await bindNotebookVault(selected)
      } else {
        setNotebookStatus('Folder selection cancelled')
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setNotebookStatus(`Folder picker failed: ${message}`)
    }
  }

  const cancelNotebookImport = () => {
    setPendingImportFiles([])
    setImportPreview(null)
    setNotebookStatus('Import cancelled')
  }

  const exportNotebookArchive = async (kind: 'backup' | 'vault') => {
    setNotebookLoading(true)
    try {
      const path = kind === 'backup'
        ? '/api/notebook/export.zip?space_id=default'
        : '/api/notebook/export-vault.zip?space_id=default'
      const res = await fetch(path)
      if (!res.ok) {
        const data = await safeJson(res)
        throw new Error(errorMessage(data, res.status))
      }
      const blob = await res.blob()
      downloadBlob(blob, kind === 'backup' ? 'notebook-export.zip' : 'notebook-vault.zip')
      setNotebookStatus(kind === 'backup' ? 'Exported notebook backup' : 'Exported Obsidian vault')
    } catch (err) {
      setNotebookStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setNotebookLoading(false)
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
            {activeTab !== 'help' && activeTab !== 'assistant' && (
              <span className="ml-auto text-sm text-gray-500">{t('settings.saved')}</span>
            )}
          </div>

          {activeTab === 'assistant' && (
            <div className="space-y-5">
              <SettingsPanel
                icon={Brain}
                title={settings.language === 'en-US' ? 'Assistant profile' : '助手配置'}
                description={settings.language === 'en-US'
                  ? 'One assistant identity is used for every new conversation. These instructions cannot override safety or data permissions.'
                  : '所有新会话使用同一个助手身份；这里的指令不能覆盖安全规则或数据权限。'}
              >
                <div className="grid gap-4">
                  <Field label={settings.language === 'en-US' ? 'Assistant name' : '助手名称'}>
                    <input className={inputClassName} value={settings.assistantName} onChange={(event) => update('assistantName', event.target.value)} />
                  </Field>
                  <Field label={settings.language === 'en-US' ? 'Behavior instructions' : '行为说明'}>
                    <textarea
                      className={`${inputClassName} min-h-28 resize-y`}
                      value={settings.assistantInstructions}
                      onChange={(event) => update('assistantInstructions', event.target.value)}
                      placeholder={settings.language === 'en-US'
                        ? 'For example: be concise, distinguish facts from suggestions, and prefer my saved terminology.'
                        : '例如：回答简洁，区分事实与建议，优先使用我笔记中的术语。'}
                    />
                  </Field>
                  <label className="flex items-start gap-3 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3">
                    <input className="mt-1" type="checkbox" checked={settings.memoryEnabled} onChange={(event) => update('memoryEnabled', event.target.checked)} />
                    <span><span className="block text-sm font-medium text-gray-900">{settings.language === 'en-US' ? 'Use long-term memory' : '使用长期记忆'}</span><span className="mt-1 block text-xs leading-5 text-gray-500">{settings.language === 'en-US' ? 'When off, new sessions neither recall nor record User Memory. Existing memory remains available here for review or deletion.' : '关闭后，新会话不会读取或记录用户记忆；已有记忆仍保留在此，供查看或删除。'}</span></span>
                  </label>
                </div>
              </SettingsPanel>
              <div className="h-[calc(100vh-20rem)] min-h-[32rem] overflow-hidden rounded-lg border border-gray-200 bg-white">
                <UserMemoryPage language={settings.language} enabled={settings.memoryEnabled} onSourceNavigate={onSourceNavigate} />
              </div>
            </div>
          )}

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
            <SettingsPanel icon={Brain} title="LLM" description="新会话会使用当前选中的对话模型服务。">
              {settings.llmConfigs.length === 0 ? (
                <EmptyConfig onAdd={addLlmConfig} label="暂无 LLM 配置" />
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
                        description="选择接口模式，并填写端点、Key 和模型 ID。"
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

                        <Field label="上下文窗口 tokens">
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
            <SettingsPanel icon={Database} title="嵌入模型" description="知识库入库和检索会使用这里的向量模型配置。">
              {settings.embeddingConfigs.length === 0 ? (
                <EmptyConfig onAdd={addEmbeddingConfig} label="暂无嵌入模型配置" />
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
                        title="嵌入接口"
                        description="配置 OpenAI-compatible embeddings 接口。"
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
            <SettingsPanel icon={Globe2} title="Search" description="Configure web search used by agent tools.">
              {settings.searchConfigs.length === 0 ? (
                <EmptyConfig onAdd={addSearchConfig} label="No search config" />
              ) : (
                <div className="grid gap-5 lg:grid-cols-[230px_1fr]">
                  <ConfigList
                    items={settings.searchConfigs.map((config) => ({
                      id: config.id,
                      title: config.name || 'DuckDuckGo',
                      subtitle: `${config.provider} · ${config.maxResults} results`,
                    }))}
                    activeId={settings.activeSearchConfigId}
                    addLabel="Add config"
                    onAdd={addSearchConfig}
                    onSelect={(id) => update('activeSearchConfigId', id)}
                  />

                  {activeSearchConfig && (
                    <div className="space-y-5 rounded-lg border border-gray-200 p-4">
                      <ConfigHeader
                        title="Web search provider"
                        description="Use free HTML providers for fallback, or configure paid search APIs for more reliable agent research."
                        onDelete={() => deleteSearchConfig(activeSearchConfig.id)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label="Config name">
                          <TextInput
                            value={activeSearchConfig.name}
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'name', value)
                            }
                          />
                        </Field>

                        <Field label="Provider">
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
                                ? 'optional'
                                : 'required'
                            }
                            onChange={(value) =>
                              updateSearchConfig(activeSearchConfig.id, 'apiKey', value)
                            }
                          />
                        </Field>

                        <Field label="Max results">
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

                        <Field label="Fetch timeout seconds">
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

                        <Field label="Max fetched characters">
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
            <SettingsPanel icon={BookMarked} title="笔记本" description="管理 Notebook 根目录、导入、导出和备份。">
              <input
                ref={importInputRef}
                className="hidden"
                type="file"
                accept=".md,.markdown,.zip,text/markdown,text/plain,application/zip"
                multiple
                onChange={(event) => {
                  void previewNotebookFiles(event.currentTarget.files)
                  event.currentTarget.value = ''
                }}
              />

              <div className="rounded-lg border border-gray-200 bg-white px-4 py-4">
                <div className="flex flex-wrap items-start gap-3">
                  <div className="flex h-9 w-9 items-center justify-center rounded-md bg-blue-50 text-blue-600">
                    <FolderOpen size={18} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-gray-900">笔记本根目录</div>
                    <div className="mt-1 break-all font-mono text-xs text-gray-500">
                      {notebookVault?.root ?? '未绑定外部 Vault，使用应用本地 Notebook 存储。'}
                    </div>
                    <div className="mt-2 flex flex-wrap gap-2 text-xs text-gray-500">
                      <span className="rounded-full bg-gray-100 px-2 py-0.5">
                        {notebookVault?.external ? 'Bound vault' : 'Local vault'}
                      </span>
                      <span className="rounded-full bg-gray-100 px-2 py-0.5">
                        {notebookVault?.entries ?? 0} notes
                      </span>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={notebookLoading}
                    onClick={() => void chooseNotebookFolder()}
                  >
                    <FolderOpen size={15} />
                    选择文件夹
                  </button>
                  <button
                    type="button"
                    className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={notebookLoading}
                    onClick={() => void refreshNotebookStatus()}
                  >
                    刷新
                  </button>
                </div>
              </div>

              <div className="grid gap-4 md:grid-cols-2">
                <section className="rounded-lg border border-gray-200 bg-white p-4">
                  <div className="flex items-start gap-3">
                    <div className="flex h-9 w-9 items-center justify-center rounded-md bg-blue-50 text-blue-600">
                      <Upload size={18} />
                    </div>
                    <div>
                      <h4 className="text-sm font-semibold text-gray-900">导入 Notebook</h4>
                      <p className="mt-1 text-sm text-gray-500">导入 Markdown 文件或 zip。桌面端推荐直接绑定 Vault 文件夹。</p>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="mt-4 inline-flex items-center gap-2 rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400"
                    disabled={notebookLoading}
                    onClick={() => importInputRef.current?.click()}
                  >
                    <Upload size={15} />
                    选择文件或 zip
                  </button>
                </section>

                <section className="rounded-lg border border-gray-200 bg-white p-4">
                  <div className="flex items-start gap-3">
                    <div className="flex h-9 w-9 items-center justify-center rounded-md bg-blue-50 text-blue-600">
                      <Download size={18} />
                    </div>
                    <div>
                      <h4 className="text-sm font-semibold text-gray-900">导出 Notebook</h4>
                      <p className="mt-1 text-sm text-gray-500">导出备份包，或导出 Obsidian 风格 Vault zip。</p>
                    </div>
                  </div>
                  <div className="mt-4 flex flex-wrap gap-2">
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={notebookLoading}
                      onClick={() => void exportNotebookArchive('backup')}
                    >
                      <Download size={15} />
                      备份 zip
                    </button>
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={notebookLoading}
                      onClick={() => void exportNotebookArchive('vault')}
                    >
                      <BookMarked size={15} />
                      Obsidian Vault
                    </button>
                  </div>
                </section>
              </div>

              {importPreview && (
                <div className="space-y-3 rounded-lg border border-blue-100 bg-blue-50/40 p-4 text-sm">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <span className="font-medium text-gray-900">
                      Import preview · {importPreview.items.length} note{importPreview.items.length === 1 ? '' : 's'}
                    </span>
                    {importPreview.conflict_count > 0 && (
                      <span className="rounded-full bg-red-50 px-2 py-0.5 text-xs font-medium text-red-600">
                        {importPreview.conflict_count} conflict{importPreview.conflict_count === 1 ? '' : 's'}
                      </span>
                    )}
                  </div>
                  <div className="max-h-52 space-y-2 overflow-y-auto pr-1">
                    {importPreview.items.map((item) => (
                      <div key={item.source_path} className="rounded-md bg-white p-2">
                        <div className="truncate font-medium text-gray-800">{item.title}</div>
                        <div className="mt-0.5 truncate text-xs text-gray-500">{item.source_path} · {item.markdown_chars} chars</div>
                        {item.duplicate_title_entry_id && (
                          <div className="mt-1 text-xs text-red-600">Same title as "{item.duplicate_title}"</div>
                        )}
                      </div>
                    ))}
                    {importPreview.skipped.map((item) => (
                      <div key={`${item.file_name}-${item.reason}`} className="rounded-md bg-red-50 p-2 text-xs text-red-600">
                        {item.file_name}: {item.reason}
                      </div>
                    ))}
                  </div>
                  <div className="flex gap-2">
                    <button
                      type="button"
                      className="inline-flex h-9 items-center justify-center rounded-md bg-blue-600 px-4 text-sm font-medium text-white hover:bg-blue-700 disabled:bg-gray-200"
                      disabled={notebookLoading || importPreview.items.length === 0}
                      onClick={() => void importNotebookFiles()}
                    >
                      导入
                    </button>
                    <button
                      type="button"
                      className="inline-flex h-9 items-center justify-center rounded-md border border-gray-200 bg-white px-4 text-sm font-medium text-gray-600 hover:bg-gray-50"
                      disabled={notebookLoading}
                      onClick={cancelNotebookImport}
                    >
                      取消
                    </button>
                  </div>
                </div>
              )}

              {importResult && importResult.skipped.length > 0 && (
                <div className="space-y-2 rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900">
                  <div className="font-medium">
                    Imported {importResult.imported_count} note{importResult.imported_count === 1 ? '' : 's'}, skipped {importResult.skipped.length}
                  </div>
                  <div className="max-h-44 space-y-1 overflow-y-auto pr-1">
                    {importResult.skipped.map((item, index) => (
                      <div key={`${item.file_name}-${item.reason}-${index}`} className="rounded-md bg-white/70 px-2 py-1.5">
                        <div className="truncate font-medium">{item.file_name}</div>
                        <div className="mt-0.5 text-xs text-amber-700">{item.reason}</div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

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
            <SettingsPanel icon={Activity} title="能力" description="预算和工具执行审批会影响新建会话。">
              <Field label="Session budget">
                <div className="flex items-center gap-3">
                  <TextInput
                    className="w-36"
                    min="0"
                    step="0.25"
                    type="number"
                    value={String(settings.budgetLimitUsd)}
                    onChange={(value) => update('budgetLimitUsd', Number(value))}
                  />
                  <span className="text-sm text-gray-600">USD</span>
                </div>
              </Field>

              <label className="flex items-center gap-3 text-sm text-gray-800">
                <input
                  className="h-4 w-4"
                  type="checkbox"
                  checked={settings.requireApproval}
                  onChange={(event) => update('requireApproval', event.target.checked)}
                />
                Require approval before tool execution
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
              onStartGuideTutor={onStartGuideTutor}
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
    assistant: 'settings.assistant.description',
    appearance: 'settings.appearance.description',
    llm: 'settings.llm.description',
    embedding: 'settings.embedding.description',
    search: 'settings.search.description',
    notebook: 'space.tabs.notebook.description',
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

function downloadBlob(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = fileName
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
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
