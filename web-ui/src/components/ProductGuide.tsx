import { useEffect, useState, type ReactNode } from 'react'
import {
  AtSign,
  Bot,
  Brain,
  Database,
  FileText,
  MessageSquare,
  Paperclip,
  SearchCheck,
  Settings2,
} from 'lucide-react'
import { useI18n } from '../i18n'
import {
  loadProductGuideState,
  productGuideTopics,
  saveProductGuideState,
  type ComposerGuideControl,
  type ProductGuideDestination,
  type ProductGuideTopic,
} from '../productGuide'
import { ComposerGuidePreview } from './ComposerGuidePreview'

interface Props {
  onNavigate: (destination: ProductGuideDestination) => void
  onStartGuideAssistant: () => void
  onRestartOnboarding: () => void
}

export function ProductGuide({ onNavigate, onStartGuideAssistant, onRestartOnboarding }: Props) {
  const { language } = useI18n()
  const copy = language === 'en-US' ? englishCopy : chineseCopy
  const [guideState, setGuideState] = useState(loadProductGuideState)

  useEffect(() => {
    saveProductGuideState(guideState)
  }, [guideState])

  const selectTopic = (topic: ProductGuideTopic) => {
    setGuideState((current) => ({ ...current, topic }))
  }
  const selectComposerControl = (composerControl: ComposerGuideControl) => {
    setGuideState({ topic: 'composer', composerControl })
  }

  return (
    <section className="min-w-0">
      <div className="flex items-center justify-between gap-4 border-b border-gray-200 pb-4">
        <p className="min-w-0 text-sm leading-5 text-gray-500">{copy.description}</p>
        <button
          type="button"
          className="inline-flex h-8 shrink-0 items-center gap-2 rounded-md border border-gray-300 bg-white px-3 text-sm font-medium text-gray-700 hover:bg-gray-50 hover:text-gray-950"
          onClick={onStartGuideAssistant}
        >
          <Bot size={15} />
          {copy.askGuide}
        </button>
      </div>

      <nav className="pt-4" aria-label={copy.topicNavigation}>
        <div className="grid grid-cols-3 gap-1 rounded-lg bg-gray-100 p-1 xl:grid-cols-6">
          {productGuideTopics.map((topic) => (
            <button
              key={topic}
              type="button"
              className={`flex h-9 min-w-0 items-center justify-center rounded-md px-2 text-sm transition-colors ${
                guideState.topic === topic
                  ? 'bg-white font-medium text-gray-950 shadow-sm'
                  : 'text-gray-500 hover:bg-white/60 hover:text-gray-900'
              }`}
              aria-current={guideState.topic === topic ? 'page' : undefined}
              onClick={() => selectTopic(topic)}
            >
              <span className="truncate">{copy.topics[topic]}</span>
            </button>
          ))}
        </div>
      </nav>

      <article className="min-h-[25rem] min-w-0 py-6">
          {guideState.topic === 'composer' && (
            <GuideSection title={copy.composer.title} description={copy.composer.description}>
              <ComposerGuidePreview
                control={guideState.composerControl}
                onControlChange={(composerControl) => setGuideState((current) => ({ ...current, composerControl }))}
              />
              <div className="mt-5 flex justify-end">
                <GuideAction onClick={() => onNavigate('chat')}>{copy.composer.openChat}</GuideAction>
              </div>
            </GuideSection>
          )}

          {guideState.topic === 'materials' && (
            <GuideSection title={copy.materials.title} description={copy.materials.description}>
              <GuideRows items={[
                { icon: <Paperclip size={18} />, title: copy.materials.attachmentTitle, text: copy.materials.attachmentText, action: copy.showInComposer, onClick: () => selectComposerControl('attachment') },
                { icon: <Database size={18} />, title: copy.materials.sourceTitle, text: copy.materials.sourceText, action: copy.showInComposer, onClick: () => selectComposerControl('source') },
                { icon: <AtSign size={18} />, title: copy.materials.mentionTitle, text: copy.materials.mentionText, action: copy.showInComposer, onClick: () => selectComposerControl('mention') },
              ]} />
            </GuideSection>
          )}

          {guideState.topic === 'modes' && (
            <GuideSection title={copy.modes.title} description={copy.modes.description}>
              <GuideRows items={copy.modes.items.map((item) => ({ ...item, onClick: () => selectComposerControl('mode') }))} />
              <div className="mt-5 flex justify-end">
                <GuideAction onClick={() => onNavigate('chat')}>{copy.modes.openChat}</GuideAction>
              </div>
            </GuideSection>
          )}

          {guideState.topic === 'knowledge' && (
            <GuideSection title={copy.knowledge.title} description={copy.knowledge.description}>
              <NumberedSteps items={copy.knowledge.steps} />
              <div className="mt-5 flex flex-wrap gap-2">
                <GuideAction onClick={() => onNavigate('embedding-settings')}>{copy.knowledge.embedding}</GuideAction>
                <GuideAction onClick={() => onNavigate('knowledge')} primary>{copy.knowledge.open}</GuideAction>
              </div>
            </GuideSection>
          )}

          {guideState.topic === 'notebook' && (
            <GuideSection title={copy.notebook.title} description={copy.notebook.description}>
              <NumberedSteps items={copy.notebook.steps} />
              <div className="mt-5 flex flex-wrap gap-2">
                <GuideAction onClick={() => onNavigate('notebook-settings')}>{copy.notebook.settings}</GuideAction>
                <GuideAction onClick={() => onNavigate('notebook')} primary>{copy.notebook.open}</GuideAction>
              </div>
            </GuideSection>
          )}

          {guideState.topic === 'memory' && (
            <GuideSection title={copy.memory.title} description={copy.memory.description}>
              <GuideRows items={copy.memory.items} />
              <div className="mt-5 flex justify-end">
                <GuideAction onClick={() => onNavigate('memory')} primary>{copy.memory.open}</GuideAction>
              </div>
            </GuideSection>
          )}

      </article>

      <footer className="flex justify-end border-t border-gray-200 pt-3 text-xs text-gray-500">
        <button type="button" className="inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 hover:bg-gray-100 hover:text-gray-900" onClick={onRestartOnboarding}>
          <Settings2 size={14} />
          {copy.restartOnboarding}
        </button>
      </footer>
    </section>
  )
}

function GuideSection({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <div>
      <h3 className="text-lg font-semibold text-gray-950">{title}</h3>
      <p className="mt-1 mb-5 text-sm leading-6 text-gray-500">{description}</p>
      {children}
    </div>
  )
}

function GuideRows({ items }: { items: Array<{ icon: ReactNode; title: string; text: string; action?: string; onClick?: () => void }> }) {
  return (
    <div className="divide-y divide-gray-100 border-y border-gray-100">
      {items.map((item) => (
        <div key={item.title} className="flex items-start gap-3 py-3.5">
          <span className="mt-0.5 text-blue-600">{item.icon}</span>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold text-gray-900">{item.title}</div>
            <p className="mt-1 text-sm leading-5 text-gray-500">{item.text}</p>
          </div>
          {item.action && item.onClick && (
            <button type="button" className="shrink-0 rounded-md px-2 py-1.5 text-xs font-medium text-blue-700 hover:bg-blue-50" onClick={item.onClick}>
              {item.action}
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

function NumberedSteps({ items }: { items: string[] }) {
  return (
    <ol className="divide-y divide-gray-100 border-y border-gray-100">
      {items.map((item, index) => (
        <li key={item} className="flex gap-3 py-3 text-sm leading-5 text-gray-600">
          <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-blue-50 text-xs font-semibold text-blue-700">{index + 1}</span>
          <span>{item}</span>
        </li>
      ))}
    </ol>
  )
}

function GuideAction({ children, onClick, primary = false }: { children: ReactNode; onClick: () => void; primary?: boolean }) {
  return (
    <button
      type="button"
      className={`inline-flex h-9 items-center rounded-md px-3 text-sm font-medium ${
        primary ? 'bg-blue-600 text-white hover:bg-blue-700' : 'border border-gray-300 bg-white text-gray-700 hover:bg-gray-50'
      }`}
      onClick={onClick}
    >
      {children}
    </button>
  )
}

const chineseCopy = {
  description: '选择主题查看对应入口和操作方式。',
  askGuide: '问使用指南',
  topicNavigation: '帮助主题',
  topics: {
    composer: '输入框控件',
    materials: '添加资料',
    modes: '会话模式',
    knowledge: '知识库',
    notebook: '笔记本',
    memory: '记忆',
  },
  showInComposer: '查看入口',
  restartOnboarding: '重新运行首次配置',
  composer: {
    title: '认识会话输入框',
    description: '点击示意输入框中的任意控件查看用途。真实聊天界面中的位置、顺序和标签与这里一致。',
    openChat: '打开真实聊天界面',
  },
  materials: {
    title: '三种添加资料的方式',
    description: '它们的生命周期和检索范围不同，不应混为同一个“上传资料”入口。',
    attachmentTitle: '附件：本条消息的临时上下文',
    attachmentText: '适合临时阅读一个文件。不会自动入库，也不会在未来会话中继续可用。',
    sourceTitle: '知识库或 Notebook：会话级资料源',
    sourceText: '适合让 Agent 在整个会话中按需检索一组资料。当前只能关联一个知识库或 Notebook。',
    mentionTitle: '@ 目标：精确引用已有内容',
    mentionText: '适合明确指定某条笔记，避免从整个资料源中猜测目标。',
  },
  modes: {
    title: '选择会话模式',
    description: '模式入口位于输入框左下角第一个按钮。模式决定交互方式，但不会替你发送第一条消息。',
    openChat: '前往聊天选择模式',
    items: [
      { icon: <MessageSquare size={18} />, title: 'Chat', text: '普通流式多轮对话，可按需使用工具，不强制启动 workflow。', action: '查看入口' },
      { icon: <SearchCheck size={18} />, title: 'Research', text: '先确认研究范围，再显式启动详细调研 workflow 并生成带引用报告。', action: '查看入口' },
      { icon: <FileText size={18} />, title: 'Organize', text: '读取 Notebook 后提出可审核的整理建议，不会绕过确认直接写入。', action: '查看入口' },
    ],
  },
  knowledge: {
    title: '配置并使用知识库',
    description: '知识库用于持久保存资料并通过向量检索为会话提供引用上下文。',
    steps: ['在“设置 > 嵌入模型”添加并测试嵌入配置。', '进入“知识库”创建知识库并添加文档。', '回到聊天，打开输入框中的资料源下拉框，选择具体知识库。', '提问后检查回答中的引用；需要精确指定现有笔记时改用 @ 目标。'],
    embedding: '配置嵌入模型',
    open: '打开知识库',
  },
  notebook: {
    title: '配置并使用 Notebook',
    description: 'Notebook 是 Markdown 工作区。可直接使用应用本地目录，也可在桌面端绑定外部 Vault。',
    steps: ['在“设置 > 笔记本”查看本地目录，或绑定一个外部 Markdown Vault。', '进入独立的“笔记”页面创建、阅读和编辑笔记。', '在聊天资料源中选择 Notebook，可让 Agent 搜索多条笔记。', '若只需要一条笔记，使用 @ 按钮精确引用。'],
    settings: 'Notebook 设置',
    open: '打开 Notebook',
  },
  memory: {
    title: '配置助手和记忆',
    description: '显式保存的长期记忆已经启用；历史检索等待 runtime 提供跨会话检索边界。',
    open: '打开记忆',
    items: [
      { icon: <Bot size={18} />, title: '助手配置', text: '设置所有新会话共用的助手名称和行为说明。' },
      { icon: <Brain size={18} />, title: '长期记忆', text: '管理事实、偏好、目标和连续性事项；写入始终由你明确发起或确认。' },
    ],
  },
}

const englishCopy: typeof chineseCopy = {
  description: 'Choose a topic to see its controls and workflow.',
  askGuide: 'Ask Usage Guide',
  topicNavigation: 'Help topics',
  topics: {
    composer: 'Composer controls',
    materials: 'Add material',
    modes: 'Conversation modes',
    knowledge: 'Knowledge Base',
    notebook: 'Notebook',
    memory: 'Memory',
  },
  showInComposer: 'Show control',
  restartOnboarding: 'Rerun first-time setup',
  composer: {
    title: 'Learn the conversation composer',
    description: 'Click any control in this composer replica. Its position, order, and label match the real Chat interface.',
    openChat: 'Open the real Chat interface',
  },
  materials: {
    title: 'Three ways to add material',
    description: 'They have different lifetimes and retrieval scopes and should not be treated as one upload feature.',
    attachmentTitle: 'Attachment: temporary context for one message',
    attachmentText: 'Use it to read a file once. It is not ingested and does not remain available to future conversations.',
    sourceTitle: 'Knowledge Base or Notebook: conversation source',
    sourceText: 'Use it when the Agent should search a collection throughout the conversation. One Knowledge Base or Notebook can be associated at a time.',
    mentionTitle: '@ target: reference exact saved content',
    mentionText: 'Use it to name one note instead of asking the Agent to infer a target from a full source.',
  },
  modes: {
    title: 'Choose a conversation mode',
    description: 'The mode entry is the first button at the lower left of the composer. It changes interaction behavior but never sends the first message for you.',
    openChat: 'Open Chat and choose a mode',
    items: [
      { icon: <MessageSquare size={18} />, title: 'Chat', text: 'Normal streaming conversation with optional tool use and no forced workflow.', action: 'Show control' },
      { icon: <SearchCheck size={18} />, title: 'Research', text: 'Clarifies scope before explicitly starting detailed research and producing a cited report.', action: 'Show control' },
      { icon: <FileText size={18} />, title: 'Organize', text: 'Reads Notebook and proposes reviewable organization changes without bypassing approval.', action: 'Show control' },
    ],
  },
  knowledge: {
    title: 'Configure and use Knowledge Base',
    description: 'Knowledge Base keeps durable material and supplies cited context through vector retrieval.',
    steps: ['Add and test an embedding profile under Settings > Embedding.', 'Open Knowledge Base, create one, and add documents.', 'Return to Chat and select that Knowledge Base from the composer source menu.', 'Ask your question and inspect citations; use an @ target when you need one exact saved item.'],
    embedding: 'Configure embedding',
    open: 'Open Knowledge Base',
  },
  notebook: {
    title: 'Configure and use Notebook',
    description: 'Notebook is a Markdown workspace. Use the app-local directory or bind an external Vault on desktop.',
    steps: ['Inspect the local directory or bind an external Markdown Vault under Settings > Notebook.', 'Create, read, and edit notes on the standalone Notebook page.', 'Select Notebook from the Chat source menu when the Agent should search multiple notes.', 'Use the @ button when you need one exact note.'],
    settings: 'Notebook settings',
    open: 'Open Notebook',
  },
  memory: {
    title: 'Configure Assistant and Memory',
    description: 'Explicit Saved Memory is available. History Recall waits for the runtime cross-session recall boundary.',
    open: 'Open Memory',
    items: [
      { icon: <Bot size={18} />, title: 'Assistant profile', text: 'Set the name and behavior instructions shared by new conversations.' },
      { icon: <Brain size={18} />, title: 'Long-term memory', text: 'Manage facts, preferences, goals, and continuity items with explicit user control.' },
    ],
  },
}
