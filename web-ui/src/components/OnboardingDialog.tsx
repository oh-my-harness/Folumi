import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  ArrowLeft,
  ArrowRight,
  Bot,
  Brain,
  Check,
  CircleCheck,
  Database,
  FolderOpen,
  HardDrive,
  NotebookPen,
  Settings2,
  Sparkles,
  X,
} from 'lucide-react'
import { activeLlmConfig, hasUsableLlmConfig, testLlmConnection, type LlmSettings } from '../settings'
import type { NotebookVaultInfo } from '../notebookSave'
import type { TutorProfile } from '../tutorTypes'
import type { OnboardingMode } from '../onboardingModes'
import { useI18n } from '../i18n'
import { TutorChooser } from './TutorChooser'
import { OnboardingModeGuide } from './OnboardingModeGuide'

interface Props {
  settings: LlmSettings
  tutors: TutorProfile[]
  knowledgeBaseCount: number
  notebookVault: NotebookVaultInfo | null
  selectedTutorId: string | null
  step: number
  onStepChange: (step: number) => void
  onTutorSelect: (tutorId: string | null) => void
  onOpenModelSettings: () => void
  onOpenEmbeddingSettings: () => void
  onOpenKnowledge: () => void
  onOpenNotebookSettings: () => void
  onOpenNotebook: () => void
  onOpenMemory: () => void
  onManageTutors: () => void
  onDismiss: () => void
  onComplete: () => void
  onStartMode: (mode: OnboardingMode) => void
}

type TestState = { status: 'idle' | 'running' | 'ok' | 'error'; message: string }
const LAST_ONBOARDING_STEP = 5

export function OnboardingDialog({
  settings,
  tutors,
  knowledgeBaseCount,
  notebookVault,
  selectedTutorId,
  step,
  onStepChange,
  onTutorSelect,
  onOpenModelSettings,
  onOpenEmbeddingSettings,
  onOpenKnowledge,
  onOpenNotebookSettings,
  onOpenNotebook,
  onOpenMemory,
  onManageTutors,
  onDismiss,
  onComplete,
  onStartMode,
}: Props) {
  const { language } = useI18n()
  const copy = language === 'en-US' ? englishCopy : chineseCopy
  const [testState, setTestState] = useState<TestState>({ status: 'idle', message: '' })
  const dialogRef = useRef<HTMLElement>(null)
  const activeModel = activeLlmConfig(settings)
  const modelReady = hasUsableLlmConfig(settings)
  const embeddingReady = settings.embeddingConfigs.some((config) => Boolean(
    config.model.trim()
    && config.baseUrl.trim()
    && config.embeddingsPath.trim()
    && config.dimensions > 0,
  ))
  const selectedTutor = useMemo(
    () => selectedTutorId ? tutors.find((tutor) => tutor.id === selectedTutorId) ?? null : null,
    [selectedTutorId, tutors],
  )

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onDismiss()
      if (event.key !== 'Tab') return
      const controls = [...(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [])]
      if (controls.length === 0) return
      const first = controls[0]
      const last = controls[controls.length - 1]
      if (!first || !last) return
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && (document.activeElement === last || !dialogRef.current?.contains(document.activeElement))) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    dialogRef.current?.querySelector<HTMLElement>('button:not([disabled])')?.focus()
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onDismiss])

  const testModel = async () => {
    if (!activeModel || !modelReady) return
    setTestState({ status: 'running', message: copy.model.testing })
    try {
      const result = await testLlmConnection(activeModel)
      setTestState({
        status: 'ok',
        message: typeof result.message === 'string' ? result.message : copy.model.testOk,
      })
    } catch (error) {
      setTestState({
        status: 'error',
        message: error instanceof Error ? error.message : copy.model.testError,
      })
    }
  }

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/30 px-5 py-6 backdrop-blur-[2px]" role="presentation">
      <section
        ref={dialogRef}
        className="flex max-h-[min(720px,calc(100vh-48px))] w-full max-w-4xl overflow-hidden rounded-lg border border-gray-200 bg-white shadow-2xl"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
      >
        <aside className="w-52 shrink-0 border-r border-gray-200 bg-gray-50 px-5 py-6">
          <div className="mb-8 flex items-center gap-2 text-gray-900">
            <Sparkles size={20} className="text-blue-600" />
            <span className="text-sm font-semibold">Folumi</span>
          </div>
          <ol className="space-y-1">
            {copy.steps.map((label, index) => (
              <li
                key={label}
                aria-current={index === step ? 'step' : undefined}
                className={`flex min-h-10 items-center gap-3 rounded-md px-3 text-sm ${
                  index === step ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'
                }`}
              >
                <span className={`flex h-5 w-5 items-center justify-center rounded-full text-[11px] ${
                  index < step
                    ? 'bg-blue-600 text-white'
                    : index === step
                      ? 'border border-blue-600 text-blue-700'
                      : 'border border-gray-300 text-gray-500'
                }`}>
                  {index < step ? <Check size={12} /> : index + 1}
                </span>
                {label}
              </li>
            ))}
          </ol>
        </aside>

        <div className="flex min-h-[520px] min-w-0 flex-1 flex-col">
          <header className="flex items-start justify-between border-b border-gray-100 px-8 py-6">
            <div>
              <h1 id="onboarding-title" className="text-xl font-semibold text-gray-950">{copy.title}</h1>
              <p className="mt-1 text-sm text-gray-500">{copy.subtitle}</p>
            </div>
            <button
              type="button"
              className="inline-flex h-9 w-9 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
              title={copy.dismiss}
              aria-label={copy.dismiss}
              onClick={onDismiss}
            >
              <X size={18} />
            </button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto px-8 py-5">
            {step === 0 && (
              <div>
                <StepHeading icon={<Settings2 size={21} />} title={copy.model.title} description={copy.model.description} />
                <div className="mt-7 rounded-md border border-gray-200 bg-gray-50 px-5 py-4">
                  <div className="flex items-center gap-3">
                    <span className={`flex h-9 w-9 items-center justify-center rounded-md ${modelReady ? 'bg-emerald-50 text-emerald-700' : 'bg-amber-50 text-amber-700'}`}>
                      {modelReady ? <CircleCheck size={20} /> : <Settings2 size={20} />}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-semibold text-gray-900">
                        {modelReady ? activeModel?.name : copy.model.missing}
                      </div>
                      <div className="mt-0.5 truncate text-xs text-gray-500">
                        {modelReady ? activeModel?.model : copy.model.missingDescription}
                      </div>
                    </div>
                    <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100" onClick={onOpenModelSettings}>
                      <Settings2 size={15} />
                      {modelReady ? copy.model.manage : copy.model.configure}
                    </button>
                  </div>
                </div>
                {modelReady && (
                  <div className="mt-4 flex items-center gap-3">
                    <button
                      type="button"
                      className="inline-flex h-9 items-center rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                      disabled={testState.status === 'running'}
                      onClick={() => void testModel()}
                    >
                      {testState.status === 'running' ? copy.model.testing : copy.model.test}
                    </button>
                    {testState.message && (
                      <span className={`text-xs ${testState.status === 'error' ? 'text-red-600' : testState.status === 'ok' ? 'text-emerald-700' : 'text-gray-500'}`}>
                        {testState.message}
                      </span>
                    )}
                  </div>
                )}
              </div>
            )}

            {step === 1 && (
              <div>
                <StepHeading icon={<Bot size={21} />} title={copy.tutor.title} description={copy.tutor.description} />
                <div className="mt-7">
                  <TutorChooser
                    tutors={tutors}
                    selectedTutorId={selectedTutorId}
                    onSelect={onTutorSelect}
                    onManage={onManageTutors}
                  />
                </div>
                <div className="mt-5 rounded-md bg-gray-50 px-4 py-3 text-sm text-gray-600">
                  {selectedTutor ? copy.tutor.selected.replace('{name}', selectedTutor.name) : copy.tutor.temporary}
                </div>
              </div>
            )}

            {step === 2 && (
              <div>
                <StepHeading icon={<Database size={21} />} title={copy.knowledge.title} description={copy.knowledge.description} />
                <GuideSteps items={copy.knowledge.instructions} />
                <div className="mt-5 flex flex-wrap items-center gap-3 rounded-md bg-gray-50 px-4 py-3">
                  <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-md ${embeddingReady ? 'bg-emerald-50 text-emerald-700' : 'bg-amber-50 text-amber-700'}`}>
                    {embeddingReady ? <CircleCheck size={19} /> : <Database size={19} />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-semibold text-gray-900">
                      {embeddingReady ? copy.knowledge.embeddingReady : copy.knowledge.embeddingMissing}
                    </div>
                    <div className="mt-0.5 text-xs text-gray-500">
                      {knowledgeBaseCount > 0
                        ? copy.knowledge.knowledgeReady.replace('{count}', String(knowledgeBaseCount))
                        : copy.knowledge.knowledgeMissing}
                    </div>
                  </div>
                  <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100" onClick={onOpenEmbeddingSettings}>
                    <Settings2 size={15} />
                    {copy.knowledge.configureEmbedding}
                  </button>
                  <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" onClick={onOpenKnowledge}>
                    <Database size={15} />
                    {copy.knowledge.open}
                  </button>
                </div>
              </div>
            )}

            {step === 3 && (
              <div>
                <StepHeading icon={<NotebookPen size={21} />} title={copy.notebook.title} description={copy.notebook.description} />
                <GuideSteps items={copy.notebook.instructions} />
                <div className="mt-5 flex flex-wrap items-center gap-3 rounded-md bg-gray-50 px-4 py-3">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700">
                    {notebookVault?.external ? <FolderOpen size={19} /> : <HardDrive size={19} />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-semibold text-gray-900">
                      {notebookVault?.external ? copy.notebook.external : copy.notebook.local}
                    </div>
                    <div className="mt-0.5 truncate text-xs text-gray-500">
                      {notebookVault?.external ? notebookVault.root : copy.notebook.localDescription}
                    </div>
                  </div>
                  <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100" onClick={onOpenNotebookSettings}>
                    <Settings2 size={15} />
                    {copy.notebook.configure}
                  </button>
                  <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" onClick={onOpenNotebook}>
                    <NotebookPen size={15} />
                    {copy.notebook.open}
                  </button>
                </div>
              </div>
            )}

            {step === 4 && (
              <div>
                <StepHeading icon={<Brain size={21} />} title={copy.memory.title} description={copy.memory.description} />
                <GuideSteps items={copy.memory.instructions} />
                <div className="mt-5 flex items-center gap-3 rounded-md bg-gray-50 px-4 py-3">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700">
                    <Brain size={19} />
                  </span>
                  <div className="min-w-0 flex-1 text-xs leading-5 text-gray-500">{copy.memory.actionDescription}</div>
                  <button type="button" className="inline-flex h-9 shrink-0 items-center gap-2 rounded-md bg-blue-600 px-3 text-sm font-medium text-white hover:bg-blue-700" onClick={onOpenMemory}>
                    <Brain size={15} />
                    {copy.memory.open}
                  </button>
                </div>
              </div>
            )}

            {step === LAST_ONBOARDING_STEP && (
              <div>
                <StepHeading icon={<Sparkles size={21} />} title={copy.mode.title} description={copy.mode.description} />
                <OnboardingModeGuide selectedTutor={selectedTutor} onStart={onStartMode} />
              </div>
            )}
          </div>

          <footer className="flex items-center border-t border-gray-100 px-8 py-4">
            <button type="button" className="rounded-md px-2 py-2 text-sm text-gray-500 hover:bg-gray-100 hover:text-gray-900" onClick={onDismiss}>{copy.later}</button>
            <div className="ml-auto flex gap-2">
              {step > 0 && (
                <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100" onClick={() => onStepChange(step - 1)}>
                  <ArrowLeft size={15} />
                  {copy.back}
                </button>
              )}
              {step < LAST_ONBOARDING_STEP && (
                <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-4 text-sm font-medium text-white hover:bg-blue-700" onClick={() => onStepChange(step + 1)}>
                  {copy.continue}
                  <ArrowRight size={15} />
                </button>
              )}
              {step === LAST_ONBOARDING_STEP && (
                <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-4 text-sm font-medium text-white hover:bg-blue-700" onClick={onComplete}>
                  <Check size={15} />
                  {copy.done}
                </button>
              )}
            </div>
          </footer>
        </div>
      </section>
    </div>
  )
}

function StepHeading({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return (
    <div className="flex gap-3">
      <span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700">{icon}</span>
      <div>
        <h2 className="text-lg font-semibold text-gray-950">{title}</h2>
        <p className="mt-1 max-w-xl text-sm leading-6 text-gray-500">{description}</p>
      </div>
    </div>
  )
}

function GuideSteps({ items }: { items: string[] }) {
  return (
    <ol className="mt-6 divide-y divide-gray-100 border-y border-gray-100">
      {items.map((item, index) => (
        <li key={item} className="flex gap-3 py-3 text-sm leading-6 text-gray-600">
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-blue-50 text-xs font-semibold text-blue-700">{index + 1}</span>
          <span>{item}</span>
        </li>
      ))}
    </ol>
  )
}

const chineseCopy = {
  title: '开始使用',
  subtitle: '完成必要配置并开始真实学习任务',
  steps: ['模型准备', '选择导师', '知识库', '笔记本', '记忆', '开始会话'],
  dismiss: '关闭使用引导',
  later: '稍后再说',
  back: '上一步',
  continue: '继续',
  done: '完成',
  model: {
    title: '确认对话模型',
    description: 'Agent 需要一个可用的模型服务。已有配置会直接复用，不会要求你重新填写凭据。',
    missing: '尚未配置模型',
    missingDescription: '你仍可浏览应用，但开始 Agent 任务前需要完成配置。',
    configure: '配置模型',
    manage: '管理',
    test: '测试连接',
    testing: '正在测试...',
    testOk: '模型连接正常。',
    testError: '模型连接失败。',
  },
  tutor: {
    title: '选择陪伴你的导师',
    description: '导师拥有稳定的 Soul 和独立记忆。也可以暂不选择，以临时助手开始一次性对话。',
    selected: '将使用“{name}”开始新会话。',
    temporary: '未选择导师，将使用临时助手。',
  },
  knowledge: {
    title: '配置并使用知识库',
    description: '知识库让 Agent 从你提供的材料中检索内容并保留引用。只有使用 RAG 时才需要配置，可以跳过。',
    instructions: [
      '先在“设置 > 嵌入模型”添加并测试 embedding 配置。',
      '进入“知识库”创建知识库，绑定嵌入模型并添加 PDF、Markdown 或文本材料。',
      '聊天时从输入框的知识库选择器挂载资料，Agent 才能按需检索并引用来源。',
    ],
    embeddingReady: '嵌入模型已配置',
    embeddingMissing: '尚未配置嵌入模型',
    knowledgeReady: '已有 {count} 个知识库可供使用。',
    knowledgeMissing: '还没有知识库；不使用 RAG 时可以直接继续。',
    configureEmbedding: '嵌入设置',
    open: '打开知识库',
  },
  notebook: {
    title: '配置你的笔记本',
    description: 'Notebook 使用 Markdown 保存研究报告、重要回答和学习笔记；默认本地目录无需额外配置。',
    instructions: [
      '直接使用应用本地 Notebook，或在“设置 > 笔记本”绑定已有 Markdown Vault。',
      '绑定外部 Vault 后，可使用系统原生保存对话框，并监听外部 Markdown 变化。',
      '进入笔记本创建文件夹和笔记，也可以从 Research 报告或消息操作栏保存内容。',
    ],
    external: '已绑定外部 Markdown Vault',
    local: '正在使用应用本地 Notebook',
    localDescription: '无需额外配置；也可以随时导入、导出或绑定外部目录。',
    configure: '笔记本设置',
    open: '打开笔记本',
  },
  memory: {
    title: '查看和更新记忆',
    description: '记忆保存的是你的学习上下文，不是外部知识资料。页面主要用于查看和维护 L2、L3。',
    instructions: [
      'L1 自动记录聊天、测验、Notebook 和知识库活动，作为后续归纳的可追溯证据。',
      'L2 按聊天、测验、笔记本和知识库整理模块摘要；L3 从 L2 综合近期状态、画像、范围、偏好和教学策略。',
      '进入 L2 或 L3 文档后，选择更新、检查或去重及模型，点击运行，再在审核视图接受并应用需要的变更。',
    ],
    actionDescription: '打开记忆概览后，可以进入 L2/L3 阅读 Markdown；仅查看引导不会自动生成或修改记忆。',
    open: '打开记忆',
  },
  mode: {
    title: '开始一段真实会话',
    description: '先认识实际输入框的各个入口，再查看模式差异。起步内容可编辑且不会自动发送。',
  },
}

const englishCopy: typeof chineseCopy = {
  title: 'Get started',
  subtitle: 'Complete essential setup and start a real learning task',
  steps: ['Model', 'Tutor', 'Knowledge', 'Notebook', 'Memory', 'Start chatting'],
  dismiss: 'Close onboarding',
  later: 'Maybe later',
  back: 'Back',
  continue: 'Continue',
  done: 'Complete',
  model: {
    title: 'Confirm a chat model',
    description: 'Agent tasks need a working model service. Existing settings are reused without asking for credentials again.',
    missing: 'No model configured',
    missingDescription: 'You can browse the app, but a model is required before starting an Agent task.',
    configure: 'Configure model',
    manage: 'Manage',
    test: 'Test connection',
    testing: 'Testing...',
    testOk: 'Model connection works.',
    testError: 'Model connection failed.',
  },
  tutor: {
    title: 'Choose a tutor',
    description: 'Tutors have a stable Soul and private continuity memory. Skip this step to use Temporary Assistant.',
    selected: 'New conversations will use “{name}”.',
    temporary: 'No tutor selected. Temporary Assistant will be used.',
  },
  knowledge: {
    title: 'Configure and use Knowledge Bases',
    description: 'Knowledge Bases let the Agent retrieve from your material with citations. This is optional when you do not need RAG.',
    instructions: [
      'Add and test an embedding configuration in Settings > Embedding.',
      'Open Knowledge Base, create one with that embedding model, and add PDF, Markdown, or text material.',
      'Select the Knowledge Base from the Chat source selector so the Agent can retrieve and cite it when needed.',
    ],
    embeddingReady: 'Embedding model configured',
    embeddingMissing: 'No embedding model configured',
    knowledgeReady: '{count} Knowledge Base(s) available.',
    knowledgeMissing: 'No Knowledge Base yet. Continue if you do not need RAG.',
    configureEmbedding: 'Embedding settings',
    open: 'Open Knowledge',
  },
  notebook: {
    title: 'Configure your Notebook',
    description: 'Notebook stores research reports, useful answers, and learning notes as Markdown. The app-local directory works without extra setup.',
    instructions: [
      'Use the app-local Notebook directly, or bind an existing Markdown Vault in Settings > Notebook.',
      'An external Vault enables the native save dialog and watches Markdown changes made outside the app.',
      'Open Notebook to create folders and notes, or save content from Research reports and message actions.',
    ],
    external: 'External Markdown Vault connected',
    local: 'Using the app-local Notebook',
    localDescription: 'No extra setup required. You can import, export, or bind an external folder later.',
    configure: 'Notebook settings',
    open: 'Open Notebook',
  },
  memory: {
    title: 'View and update Memory',
    description: 'Memory stores your learning context rather than external reference material. The page focuses on visible L2 and L3 maintenance.',
    instructions: [
      'L1 automatically records Chat, Quiz, Notebook, and Knowledge activity as traceable evidence for later consolidation.',
      'L2 organizes per-module summaries; L3 synthesizes recent state, profile, scope, preferences, and teaching strategy from L2.',
      'Open an L2 or L3 document, choose Update, Check, or Deduplicate and a model, run the workflow, then review and apply accepted changes.',
    ],
    actionDescription: 'Open Memory to read L2/L3 Markdown. Viewing this guide never generates or modifies memory by itself.',
    open: 'Open Memory',
  },
  mode: {
    title: 'Start a real conversation',
    description: 'Learn the actual composer controls, then review mode behavior. Starter prompts stay editable and are never sent automatically.',
  },
}
