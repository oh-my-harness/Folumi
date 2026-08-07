import { useEffect, useState, type ReactNode } from 'react'
import {
  AtSign,
  Bot,
  Brain,
  Database,
  Paperclip,
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
  description: '选择一个主题，快速了解怎么用。',
  askGuide: '问使用指南',
  topicNavigation: '帮助主题',
  topics: {
    composer: '输入框控件',
    materials: '添加资料',
    knowledge: '资料',
    notebook: '笔记本',
    memory: '个性化',
  },
  showInComposer: '查看入口',
  restartOnboarding: '重新运行首次配置',
  composer: {
    title: '认识会话输入框',
    description: '点击输入框中的按钮查看用途。',
    openChat: '打开真实聊天界面',
  },
  materials: {
    title: '三种添加资料的方式',
    description: '根据文件要用多久，选择合适的方式。',
    attachmentTitle: '附件：只用于这条消息',
    attachmentText: '适合临时阅读文件，不会保存到资料或笔记。',
    sourceTitle: '资料或笔记：用于整次聊天',
    sourceText: '适合在聊天中反复参考一组内容。一次选择一个来源。',
    mentionTitle: '@ 笔记：指定一篇内容',
    mentionText: '适合直接告诉助手要看哪篇笔记。',
  },
  knowledge: {
    title: '配置并使用资料',
    description: '把需要参考的文件整理在一起，聊天时可以随时使用。',
    steps: ['在“设置 > 嵌入模型”完成配置。', '进入“资料”创建资料集并添加文件。', '回到聊天，从输入框中选择资料集。', '提问后可以打开回答中的出处。'],
    embedding: '配置嵌入模型',
    open: '打开资料',
  },
  notebook: {
    title: '配置并使用笔记',
    description: '写下、整理并连接自己的想法。',
    steps: ['在“设置 > 笔记本”选择笔记保存位置。', '进入“笔记”页面创建和编辑内容。', '聊天时选择“笔记”，即可查找多篇内容。', '只需要一篇时，使用 @ 按钮指定。'],
    settings: '笔记本设置',
    open: '打开笔记',
  },
  memory: {
    title: '设置个性化体验',
    description: '你可以决定助手了解哪些个人信息、何时参考过往对话，以及助手如何表现自己。',
    open: '打开个性化',
    items: [
      { icon: <Brain size={18} />, title: '关于我', text: '管理助手记住的信息和过往对话。' },
      { icon: <Bot size={18} />, title: '助手设定', text: '设置助手的名字和回答方式。' },
    ],
  },
}

const englishCopy: typeof chineseCopy = {
  description: 'Choose a topic for a quick explanation.',
  askGuide: 'Ask Usage Guide',
  topicNavigation: 'Help topics',
  topics: {
    composer: 'Composer controls',
    materials: 'Add material',
    knowledge: 'Sources',
    notebook: 'Notebook',
    memory: 'Personalization',
  },
  showInComposer: 'Show control',
  restartOnboarding: 'Rerun first-time setup',
  composer: {
    title: 'Learn the conversation composer',
    description: 'Click a composer control to see what it does.',
    openChat: 'Open the real Chat interface',
  },
  materials: {
    title: 'Three ways to add material',
    description: 'Choose based on how long you need the file.',
    attachmentTitle: 'Attachment: this message only',
    attachmentText: 'Use it to read a file once. It is not saved to Sources or Notebook.',
    sourceTitle: 'Sources or Notebook: the whole conversation',
    sourceText: 'Use a collection repeatedly throughout a conversation. Choose one source at a time.',
    mentionTitle: '@ note: choose one note',
    mentionText: 'Use it to tell the Assistant exactly which note to read.',
  },
  knowledge: {
    title: 'Configure and use Sources',
    description: 'Keep reference files together and use them while chatting.',
    steps: ['Complete Settings > Embedding models.', 'Create a collection under Sources and add files.', 'Return to Chat and choose that collection.', 'Open citations in an answer to see where it came from.'],
    embedding: 'Configure embedding model',
    open: 'Open Sources',
  },
  notebook: {
    title: 'Configure and use Notebook',
    description: 'Write, organize, and connect your own ideas.',
    steps: ['Choose where notes are saved under Settings > Notebook.', 'Create and edit content on the Notebook page.', 'Choose Notebook in Chat to search across notes.', 'Use @ when you need one exact note.'],
    settings: 'Notebook settings',
    open: 'Open Notebook',
  },
  memory: {
    title: 'Configure Personalization',
    description: 'Choose what the Assistant knows about you, when it may reference past conversations, and how it presents itself.',
    open: 'Open Personalization',
    items: [
      { icon: <Brain size={18} />, title: 'About me', text: 'Manage remembered information and past conversations.' },
      { icon: <Bot size={18} />, title: 'Assistant setup', text: 'Choose the Assistant’s name and response style.' },
    ],
  },
}
