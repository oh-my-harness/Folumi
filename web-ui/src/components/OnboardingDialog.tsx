import { useEffect, useRef, useState, type ReactNode } from 'react'
import { ArrowLeft, ArrowRight, Check, CircleCheck, Database, MessageSquare, Settings2, Sparkles, X } from 'lucide-react'
import { activeLlmConfig, hasUsableLlmConfig, testLlmConnection, type LlmSettings } from '../settings'
import { useI18n } from '../i18n'

interface Props {
  settings: LlmSettings
  knowledgeBaseCount: number
  step: number
  onStepChange: (step: number) => void
  onOpenModelSettings: () => void
  onOpenEmbeddingSettings: () => void
  onOpenKnowledge: () => void
  onDismiss: () => void
  onComplete: () => void
  onStart: () => void
}

type TestState = { status: 'idle' | 'running' | 'ok' | 'error'; message: string }
const LAST_STEP = 2

export function OnboardingDialog({
  settings,
  knowledgeBaseCount,
  step,
  onStepChange,
  onOpenModelSettings,
  onOpenEmbeddingSettings,
  onOpenKnowledge,
  onDismiss,
  onComplete,
  onStart,
}: Props) {
  const { language } = useI18n()
  const copy = language === 'en-US' ? englishCopy : chineseCopy
  const [testState, setTestState] = useState<TestState>({ status: 'idle', message: '' })
  const dialogRef = useRef<HTMLElement>(null)
  const activeModel = activeLlmConfig(settings)
  const modelReady = hasUsableLlmConfig(settings)
  const embeddingReady = settings.embeddingConfigs.some((config) => Boolean(
    config.model.trim() && config.baseUrl.trim() && config.embeddingsPath.trim() && config.dimensions > 0,
  ))

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onDismiss()
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
      setTestState({ status: 'ok', message: typeof result.message === 'string' ? result.message : copy.model.testOk })
    } catch (error) {
      setTestState({ status: 'error', message: error instanceof Error ? error.message : copy.model.testError })
    }
  }

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/30 px-5 py-6 backdrop-blur-[2px]" role="presentation">
      <section ref={dialogRef} className="flex max-h-[min(680px,calc(100vh-48px))] w-full max-w-4xl overflow-hidden rounded-lg border border-gray-200 bg-white shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
        <aside className="w-52 shrink-0 border-r border-gray-200 bg-gray-50 px-5 py-6">
          <div className="mb-8 flex items-center gap-2 text-gray-900">
            <Sparkles size={20} className="text-blue-600" />
            <span className="text-sm font-semibold">Folumi</span>
          </div>
          <ol className="space-y-1">
            {copy.steps.map((label, index) => (
              <li key={label} aria-current={index === step ? 'step' : undefined} className={`flex min-h-10 items-center gap-3 rounded-md px-3 text-sm ${index === step ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'}`}>
                <span className={`flex h-5 w-5 items-center justify-center rounded-full text-[11px] ${index < step ? 'bg-blue-600 text-white' : index === step ? 'border border-blue-600 text-blue-700' : 'border border-gray-300 text-gray-500'}`}>
                  {index < step ? <Check size={12} /> : index + 1}
                </span>
                {label}
              </li>
            ))}
          </ol>
        </aside>

        <div className="flex min-h-[500px] min-w-0 flex-1 flex-col">
          <header className="flex items-start justify-between border-b border-gray-100 px-8 py-6">
            <div>
              <h1 id="onboarding-title" className="text-xl font-semibold text-gray-950">{copy.title}</h1>
              <p className="mt-1 text-sm text-gray-500">{copy.subtitle}</p>
            </div>
            <button type="button" className="inline-flex h-9 w-9 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100" aria-label={copy.dismiss} onClick={onDismiss}><X size={18} /></button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
            {step === 0 && (
              <div>
                <StepHeading icon={<Settings2 size={21} />} title={copy.model.title} description={copy.model.description} />
                <div className="mt-7 flex items-center gap-3 rounded-md border border-gray-200 bg-gray-50 px-5 py-4">
                  <span className={`flex h-9 w-9 items-center justify-center rounded-md ${modelReady ? 'bg-emerald-50 text-emerald-700' : 'bg-amber-50 text-amber-700'}`}>{modelReady ? <CircleCheck size={20} /> : <Settings2 size={20} />}</span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-semibold text-gray-900">{modelReady ? activeModel?.name : copy.model.missing}</div>
                    <div className="mt-0.5 truncate text-xs text-gray-500">{modelReady ? activeModel?.model : copy.model.missingDescription}</div>
                  </div>
                  <button type="button" className="h-9 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-100" onClick={onOpenModelSettings}>{modelReady ? copy.model.manage : copy.model.configure}</button>
                </div>
                {modelReady && <div className="mt-4 flex items-center gap-3"><button type="button" className="h-9 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 disabled:opacity-50" disabled={testState.status === 'running'} onClick={() => void testModel()}>{testState.status === 'running' ? copy.model.testing : copy.model.test}</button>{testState.message && <span className={`text-xs ${testState.status === 'error' ? 'text-red-600' : 'text-emerald-700'}`}>{testState.message}</span>}</div>}
              </div>
            )}

            {step === 1 && (
              <div>
                <StepHeading icon={<Database size={21} />} title={copy.knowledge.title} description={copy.knowledge.description} />
                <ol className="mt-6 divide-y divide-gray-100 border-y border-gray-100">
                  {copy.knowledge.instructions.map((item, index) => <li key={item} className="flex gap-3 py-3 text-sm leading-6 text-gray-600"><span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-blue-50 text-xs font-semibold text-blue-700">{index + 1}</span>{item}</li>)}
                </ol>
                <div className="mt-5 flex flex-wrap items-center gap-3 rounded-md bg-gray-50 px-4 py-3">
                  <div className="min-w-0 flex-1 text-sm text-gray-600">{embeddingReady ? copy.knowledge.embeddingReady : copy.knowledge.embeddingMissing} · {knowledgeBaseCount ? copy.knowledge.ready.replace('{count}', String(knowledgeBaseCount)) : copy.knowledge.empty}</div>
                  <button type="button" className="h-9 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700" onClick={onOpenEmbeddingSettings}>{copy.knowledge.embedding}</button>
                  <button type="button" className="h-9 rounded-md bg-blue-600 px-3 text-sm font-medium text-white" onClick={onOpenKnowledge}>{copy.knowledge.open}</button>
                </div>
              </div>
            )}

            {step === LAST_STEP && (
              <div>
                <StepHeading icon={<MessageSquare size={21} />} title={copy.start.title} description={copy.start.description} />
                <div className="mt-7 rounded-lg border border-blue-100 bg-blue-50/60 p-5">
                  <p className="text-sm leading-6 text-gray-700">{copy.start.example}</p>
                  <button type="button" className="mt-5 inline-flex h-10 items-center gap-2 rounded-md bg-blue-600 px-4 text-sm font-medium text-white hover:bg-blue-700" onClick={onStart}><Sparkles size={16} />{copy.start.action}</button>
                </div>
              </div>
            )}
          </div>

          <footer className="flex items-center border-t border-gray-100 px-8 py-4">
            <button type="button" className="rounded-md px-2 py-2 text-sm text-gray-500 hover:bg-gray-100" onClick={onDismiss}>{copy.later}</button>
            <div className="ml-auto flex gap-2">
              {step > 0 && <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700" onClick={() => onStepChange(step - 1)}><ArrowLeft size={15} />{copy.back}</button>}
              {step < LAST_STEP && <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-4 text-sm font-medium text-white" onClick={() => onStepChange(step + 1)}>{copy.continue}<ArrowRight size={15} /></button>}
              {step === LAST_STEP && <button type="button" className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-4 text-sm font-medium text-gray-700" onClick={onComplete}><Check size={15} />{copy.done}</button>}
            </div>
          </footer>
        </div>
      </section>
    </div>
  )
}

function StepHeading({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return <div className="flex gap-3"><span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700">{icon}</span><div><h2 className="text-lg font-semibold text-gray-950">{title}</h2><p className="mt-1 max-w-xl text-sm leading-6 text-gray-500">{description}</p></div></div>
}

const chineseCopy = {
  title: '开始使用 Folumi', subtitle: '准备模型、加入资料，然后开始第一次有来源的问答', steps: ['准备模型', '加入知识', '开始提问'], dismiss: '关闭使用引导', later: '稍后再说', back: '上一步', continue: '继续', done: '完成',
  model: { title: '准备对话模型', description: 'Folumi 需要一个可用的模型来回答问题；已有配置会直接复用。', missing: '尚未配置模型', missingDescription: '可以继续浏览，但提问前需要完成模型配置。', configure: '配置模型', manage: '管理', test: '测试连接', testing: '正在测试…', testOk: '模型连接正常。', testError: '模型连接失败。' },
  knowledge: { title: '加入你的知识', description: '来源文档和个人笔记统一放在知识库中；不需要检索时也可以跳过。', instructions: ['导入 PDF、Markdown 或文本作为可追溯的来源。', '创建或导入 Markdown 笔记，保存自己的理解。', '聊天时关联来源、笔记或临时附件，回答会保留引用。'], embeddingReady: '嵌入模型已配置', embeddingMissing: '尚未配置嵌入模型', ready: '已有 {count} 个知识库', empty: '还没有来源', embedding: '嵌入设置', open: '打开知识库' },
  start: { title: '开始第一次提问', description: '无需选择导师或工作流。直接描述问题，需要深入检索时再点击“研究”。', example: '例如：“根据我刚导入的项目资料，总结三个关键结论，并标明分别来自哪里。”', action: '去问助手' },
}

const englishCopy: typeof chineseCopy = {
  title: 'Get started with Folumi', subtitle: 'Prepare a model, add material, and ask your first grounded question', steps: ['Model', 'Knowledge', 'Ask'], dismiss: 'Close onboarding', later: 'Maybe later', back: 'Back', continue: 'Continue', done: 'Complete',
  model: { title: 'Prepare a chat model', description: 'Folumi needs a working model to answer questions. Existing settings are reused.', missing: 'No model configured', missingDescription: 'You can browse now, but a model is required before asking.', configure: 'Configure', manage: 'Manage', test: 'Test connection', testing: 'Testing…', testOk: 'Model connection works.', testError: 'Model connection failed.' },
  knowledge: { title: 'Add your knowledge', description: 'Source documents and personal notes live together in Knowledge Base. Skip this when retrieval is unnecessary.', instructions: ['Import PDF, Markdown, or text as traceable Sources.', 'Create or import Markdown Notes for your own understanding.', 'Attach Sources, Notes, or temporary files in chat and keep citations in the answer.'], embeddingReady: 'Embedding configured', embeddingMissing: 'No embedding configured', ready: '{count} knowledge base(s)', empty: 'No sources yet', embedding: 'Embedding settings', open: 'Open Knowledge Base' },
  start: { title: 'Ask your first question', description: 'No tutor or workflow selection is required. Describe the task and use Research only when you need deeper investigation.', example: 'For example: “Summarize the three key conclusions in my project material and cite where each one came from.”', action: 'Ask Assistant' },
}
