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
  | 'chat.knowledge.use.description'
  | 'chat.notebook.description'
  | 'chat.source.select'
  | 'chat.source.selected'
  | 'chat.source.menu.title'
  | 'chat.source.menu.description'
  | 'chat.model.menu.title'
  | 'chat.model.menu.description'
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
    'settings.notebook.description': '管理笔记位置、导入和备份。',
    'chat.title': '助手',
    'chat.subtitle': '和助手对话、查资料、整理想法。',
    'chat.new': '新对话',
    'chat.empty.title': '今天想了解什么？',
    'chat.empty.description': '直接提问，也可以带上资料或笔记。',
    'chat.input.placeholder': '今天我能帮您什么？',
    'chat.attachments': '附件',
    'chat.knowledge.use.description': '从这个资料集中查找答案',
    'chat.notebook.description': '从你的笔记中查找内容',
    'chat.source.select': '关联资料',
    'chat.source.selected': '已选',
    'chat.source.menu.title': '选择参考内容',
    'chat.source.menu.description': '点击选择，再次点击取消',
    'chat.model.menu.title': '选择模型',
    'chat.model.menu.description': '选择回答所用的模型',
    'chat.model.select': '选择模型',
    'chat.model.none': '暂无模型配置',
    'chat.model.configureFirst': '请先到设置中添加对话模型',
    'chat.send': '发送',
    'chat.stop': '停止生成',
    'chat.temporary.sidebar': '临时对话 · 不使用个人信息或过往对话',
    'settings.title': '设置',
    'settings.subtitle': '按你的习惯调整 Folumi。',
    'settings.saved': '所有更改已保存',
    'settings.tabs.appearance': '外观',
    'settings.tabs.llm': 'LLM',
    'settings.tabs.embedding': '嵌入模型',
    'settings.tabs.search': '联网搜索',
    'settings.tabs.governance': '权限与数据',
    'settings.tabs.help': '帮助',
    'settings.appearance.title': '界面外观',
    'settings.appearance.description': '调整语言和主题。',
    'settings.theme.title': '主题色',
    'settings.theme.description': '选择你喜欢的界面颜色。',
    'settings.theme.coolLight': '冷灰浅色',
    'settings.theme.coolLight.description': '明亮清爽，适合白天使用。',
    'settings.theme.graphiteDark': '石墨深色',
    'settings.theme.graphiteDark.description': '柔和低眩光，适合夜间使用。',
    'settings.llm.description': '添加和选择回答问题的模型。',
    'settings.embedding.description': '设置资料查找所需的模型。',
    'settings.search.description': '设置助手使用的网页搜索服务。',
    'settings.governance.description': '管理操作确认和本地数据。',
    'settings.help.description': '查看入门说明和常用操作。',
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
    'settings.notebook.description': 'Manage note storage, imports, and backups.',
    'chat.title': 'Assistant',
    'chat.subtitle': 'Chat, find information, and organize ideas.',
    'chat.new': 'New chat',
    'chat.empty.title': 'What would you like to understand?',
    'chat.empty.description': 'Ask directly, with sources or notes when useful.',
    'chat.input.placeholder': 'How can I help today?',
    'chat.attachments': 'Attachments',
    'chat.knowledge.use.description': 'Find answers in this source collection',
    'chat.notebook.description': 'Find content in your notes',
    'chat.source.select': 'Sources',
    'chat.source.selected': 'Selected',
    'chat.source.menu.title': 'Choose references',
    'chat.source.menu.description': 'Click to select; click again to remove',
    'chat.model.menu.title': 'Choose a model',
    'chat.model.menu.description': 'Choose the model that answers',
    'chat.model.select': 'Select model',
    'chat.model.none': 'No model profiles',
    'chat.model.configureFirst': 'Add a chat model in Settings first',
    'chat.send': 'Send',
    'chat.stop': 'Stop generation',
    'chat.temporary.sidebar': 'Temporary · no personal info or past conversations',
    'settings.title': 'Settings',
    'settings.subtitle': 'Make Folumi work the way you prefer.',
    'settings.saved': 'All changes saved',
    'settings.tabs.appearance': 'Appearance',
    'settings.tabs.llm': 'LLM',
    'settings.tabs.embedding': 'Embedding models',
    'settings.tabs.search': 'Web search',
    'settings.tabs.governance': 'Permissions & data',
    'settings.tabs.help': 'Help',
    'settings.appearance.title': 'Appearance',
    'settings.appearance.description': 'Choose a language and theme.',
    'settings.theme.title': 'Color theme',
    'settings.theme.description': 'Choose the colors you prefer.',
    'settings.theme.coolLight': 'Cool Light',
    'settings.theme.coolLight.description': 'Bright and crisp for daytime use.',
    'settings.theme.graphiteDark': 'Graphite Dark',
    'settings.theme.graphiteDark.description': 'Soft and low-glare for nighttime use.',
    'settings.llm.description': 'Add and choose models for conversations.',
    'settings.embedding.description': 'Set up the model used to search sources.',
    'settings.search.description': 'Choose a service for searching the web.',
    'settings.governance.description': 'Manage confirmations and local data.',
    'settings.help.description': 'View getting-started help and common tasks.',
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
