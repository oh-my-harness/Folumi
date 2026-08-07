import { createContext, useContext, type ReactNode } from 'react'

export type UiLanguage = 'zh-CN' | 'en-US'

export type TranslationKey =
  | 'app.subtitle'
  | 'nav.assistant'
  | 'nav.chat'
  | 'nav.knowledge'
  | 'nav.notebook'
  | 'nav.memory'
  | 'nav.settings'
  | 'nav.recent'
  | 'nav.noRecent'
  | 'nav.collapse'
  | 'nav.expand'
  | 'settings.tabs.notebook'
  | 'settings.notebook.description'
  | 'chat.title'
  | 'chat.subtitle'
  | 'chat.new'
  | 'chat.empty.title'
  | 'chat.empty.description'
  | 'chat.input.placeholder'
  | 'chat.attachments'
  | 'chat.knowledge.none'
  | 'chat.knowledge.none.description'
  | 'chat.knowledge.use.description'
  | 'chat.notebook.description'
  | 'chat.source.menu.title'
  | 'chat.source.menu.description'
  | 'chat.notes.menu.title'
  | 'chat.notes.menu.description'
  | 'chat.model.menu.title'
  | 'chat.model.menu.description'
  | 'chat.notes.searchPlaceholder'
  | 'chat.notes.updating'
  | 'chat.notes.noMatching'
  | 'chat.model.select'
  | 'chat.model.none'
  | 'chat.model.configureFirst'
  | 'chat.send'
  | 'chat.stop'
  | 'chat.temporary.sidebar'
  | 'settings.title'
  | 'settings.subtitle'
  | 'settings.saved'
  | 'settings.tabs.appearance'
  | 'settings.tabs.llm'
  | 'settings.tabs.embedding'
  | 'settings.tabs.search'
  | 'settings.tabs.governance'
  | 'settings.tabs.help'
  | 'settings.appearance.title'
  | 'settings.appearance.description'
  | 'settings.theme.title'
  | 'settings.theme.description'
  | 'settings.theme.coolLight'
  | 'settings.theme.coolLight.description'
  | 'settings.theme.graphiteDark'
  | 'settings.theme.graphiteDark.description'
  | 'settings.llm.description'
  | 'settings.embedding.description'
  | 'settings.search.description'
  | 'settings.governance.description'
  | 'settings.help.description'
  | 'settings.language.title'
  | 'settings.language.description.zh'
  | 'settings.language.description.en'
  | 'settings.language.english'
  | 'settings.language.chinese'

const translations: Record<UiLanguage, Record<TranslationKey, string>> = {
  'zh-CN': {
    'app.subtitle': '本地个人知识助手',
    'nav.assistant': '助手',
    'nav.chat': '聊天',
    'nav.knowledge': '资料',
    'nav.notebook': '笔记',
    'nav.memory': '个性化',
    'nav.settings': '设置',
    'nav.recent': '最近',
    'nav.noRecent': '暂无历史会话',
    'nav.collapse': '收起侧边栏',
    'nav.expand': '展开侧边栏',
    'settings.tabs.notebook': '笔记本',
    'settings.notebook.description': '保存研究报告、笔记、片段和可复用记录。',
    'chat.title': '助手',
    'chat.subtitle': '基于你的资料提问、研究和整理想法。',
    'chat.new': '新对话',
    'chat.empty.title': '今天想了解什么？',
    'chat.empty.description': '直接提问，或关联参考资料和 Notebook 笔记，让回答有据可查。',
    'chat.input.placeholder': '今天我能帮您什么？',
    'chat.attachments': '附件',
    'chat.knowledge.none': '不关联资料',
    'chat.knowledge.none.description': '仅使用当前对话上下文',
    'chat.knowledge.use.description': '关联此资料集进行检索',
    'chat.notebook.description': '搜索你创建和管理的 Markdown 笔记',
    'chat.source.menu.title': '选择资料范围',
    'chat.source.menu.description': '用于整个会话，可随时切换',
    'chat.notes.menu.title': '引用笔记',
    'chat.notes.menu.description': '精确指定本次消息需要读取的内容',
    'chat.model.menu.title': '选择模型',
    'chat.model.menu.description': '决定新消息使用哪个模型服务',
    'chat.notes.searchPlaceholder': '搜索笔记...',
    'chat.notes.updating': '更新中...',
    'chat.notes.noMatching': '没有匹配内容。',
    'chat.model.select': '选择模型',
    'chat.model.none': '暂无模型配置',
    'chat.model.configureFirst': '请先到设置中添加 LLM 配置',
    'chat.send': '发送',
    'chat.stop': '停止生成',
    'chat.temporary.sidebar': '临时对话 · 不使用个人信息或过往对话',
    'settings.title': '设置',
    'settings.subtitle': '调整外观、配置模型服务、查看内置工具。',
    'settings.saved': '所有更改已保存',
    'settings.tabs.appearance': '外观',
    'settings.tabs.llm': 'LLM',
    'settings.tabs.embedding': '嵌入模型',
    'settings.tabs.search': '搜索',
    'settings.tabs.governance': '能力',
    'settings.tabs.help': '帮助',
    'settings.appearance.title': '界面外观',
    'settings.appearance.description': '调整界面语言和视觉偏好。',
    'settings.theme.title': '主题色',
    'settings.theme.description': '选择应用框架、工作区和内容表面的整体配色。',
    'settings.theme.coolLight': '冷灰浅色',
    'settings.theme.coolLight.description': '冷灰框架、白色内容面和清晰的中性层级。',
    'settings.theme.graphiteDark': '石墨深色',
    'settings.theme.graphiteDark.description': '中性石墨背景、柔和白字和低眩光内容面。',
    'settings.llm.description': '配置对话模型服务，可新增多个服务配置。',
    'settings.embedding.description': '配置资料入库和检索使用的嵌入模型。',
    'settings.search.description': '配置 agent web_search 工具使用的搜索服务。',
    'settings.governance.description': '配置工具执行审批和本地运行选项。',
    'settings.help.description': '使用引导和常用帮助入口。',
    'settings.language.title': '界面语言',
    'settings.language.description.zh': '当前使用中文界面。',
    'settings.language.description.en': '当前使用英文界面。',
    'settings.language.english': 'English',
    'settings.language.chinese': '中文',
  },
  'en-US': {
    'app.subtitle': 'Personal knowledge assistant',
    'nav.assistant': 'Assistant',
    'nav.chat': 'Chat',
    'nav.knowledge': 'Sources',
    'nav.notebook': 'Notebook',
    'nav.memory': 'Personalization',
    'nav.settings': 'Settings',
    'nav.recent': 'Recent',
    'nav.noRecent': 'No recent sessions',
    'nav.collapse': 'Collapse sidebar',
    'nav.expand': 'Expand sidebar',
    'settings.tabs.notebook': 'Notebook',
    'settings.notebook.description': 'Saved reports, notes, snippets, and reusable records.',
    'chat.title': 'Assistant',
    'chat.subtitle': 'Ask, research, and organize ideas using your material.',
    'chat.new': 'New chat',
    'chat.empty.title': 'What would you like to understand?',
    'chat.empty.description': 'Ask directly, or attach reference sources and Notebook notes for grounded answers.',
    'chat.input.placeholder': 'How can I help today?',
    'chat.attachments': 'Attachments',
    'chat.knowledge.none': 'No sources',
    'chat.knowledge.none.description': 'Use only the current conversation context',
    'chat.knowledge.use.description': 'Search this source collection for context',
    'chat.notebook.description': 'Search the Markdown notes you create and manage',
    'chat.source.menu.title': 'Choose a source',
    'chat.source.menu.description': 'Applies to the conversation and can be changed anytime',
    'chat.notes.menu.title': 'Reference a note',
    'chat.notes.menu.description': 'Choose exact content for this message',
    'chat.model.menu.title': 'Choose a model',
    'chat.model.menu.description': 'Select the model service for new messages',
    'chat.notes.searchPlaceholder': 'Search notes...',
    'chat.notes.updating': 'Updating...',
    'chat.notes.noMatching': 'No matching content.',
    'chat.model.select': 'Select model',
    'chat.model.none': 'No model profiles',
    'chat.model.configureFirst': 'Add an LLM profile in Settings first',
    'chat.send': 'Send',
    'chat.stop': 'Stop generation',
    'chat.temporary.sidebar': 'Temporary · no personal info or past conversations',
    'settings.title': 'Settings',
    'settings.subtitle': 'Adjust appearance, configure model services, and inspect built-in tools.',
    'settings.saved': 'All changes saved',
    'settings.tabs.appearance': 'Appearance',
    'settings.tabs.llm': 'LLM',
    'settings.tabs.embedding': 'Embedding',
    'settings.tabs.search': 'Search',
    'settings.tabs.governance': 'Capabilities',
    'settings.tabs.help': 'Help',
    'settings.appearance.title': 'Appearance',
    'settings.appearance.description': 'Adjust interface language and visual preferences.',
    'settings.theme.title': 'Color theme',
    'settings.theme.description': 'Choose the palette used by the app frame, workspaces, and content surfaces.',
    'settings.theme.coolLight': 'Cool Light',
    'settings.theme.coolLight.description': 'Cool-gray framing, white content surfaces, and crisp neutral layers.',
    'settings.theme.graphiteDark': 'Graphite Dark',
    'settings.theme.graphiteDark.description': 'Neutral graphite framing, soft white text, and low-glare content surfaces.',
    'settings.llm.description': 'Configure chat model services and add multiple service profiles.',
    'settings.embedding.description': 'Configure embedding models used to index and retrieve sources.',
    'settings.search.description': 'Configure search services used by the agent web_search tool.',
    'settings.governance.description': 'Configure tool approval and local runtime options.',
    'settings.help.description': 'Getting-started guidance and common help actions.',
    'settings.language.title': 'Interface language',
    'settings.language.description.zh': 'Chinese interface is active.',
    'settings.language.description.en': 'English interface is active.',
    'settings.language.english': 'English',
    'settings.language.chinese': '中文',
  },
}

const I18nContext = createContext<{
  language: UiLanguage
  t: (key: TranslationKey) => string
}>({
  language: 'zh-CN',
  t: (key) => translations['zh-CN'][key],
})

export function translate(language: UiLanguage, key: TranslationKey) {
  const normalizedLanguage = language === 'en-US' ? 'en-US' : 'zh-CN'
  return translations[normalizedLanguage][key]
}

export function I18nProvider({
  language,
  children,
}: {
  language: UiLanguage
  children: ReactNode
}) {
  const normalizedLanguage = language === 'en-US' ? 'en-US' : 'zh-CN'
  return (
    <I18nContext.Provider
      value={{
        language: normalizedLanguage,
        t: (key) => translations[normalizedLanguage][key],
      }}
    >
      {children}
    </I18nContext.Provider>
  )
}

export function useI18n() {
  return useContext(I18nContext)
}
