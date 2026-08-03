import { useState } from 'react'
import { Bot, Brain } from 'lucide-react'
import type { SourceReference, SourceTarget } from './MarkdownMessage'

interface Props {
  language: 'zh-CN' | 'en-US'
  assistantName: string
  assistantInstructions: string
  onAssistantProfileChange: (profile: { name: string; instructions: string }) => void
  onSourceNavigate?: (target: SourceTarget, reference: SourceReference) => void
}

export function UserMemoryPage({
  language,
  assistantName,
  assistantInstructions,
  onAssistantProfileChange,
}: Props) {
  const [activeTab, setActiveTab] = useState<'memory' | 'assistant'>('memory')

  return (
    <section className="h-full overflow-y-auto bg-white px-6 py-6">
      <div className="flex items-start gap-3 border-b border-gray-200 pb-5">
        <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700"><Brain size={21} /></span>
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-semibold text-gray-950">{language === 'en-US' ? 'Memory' : '记忆'}</h1>
          <p className="mt-1 text-sm text-gray-500">{language === 'en-US' ? 'Configure the assistant and manage durable context.' : '配置助手，并管理跨会话延续的信息。'}</p>
        </div>
      </div>

      <div className="mt-5 flex gap-1 border-b border-gray-200" role="tablist" aria-label={language === 'en-US' ? 'Memory sections' : '记忆页面分区'}>
        <button type="button" role="tab" id="memory-tab" aria-controls="memory-panel" aria-selected={activeTab === 'memory'} className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${activeTab === 'memory' ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`} onClick={() => setActiveTab('memory')}>
          <Brain size={16} />{language === 'en-US' ? 'Long-term memory' : '长期记忆'}
        </button>
        <button type="button" role="tab" id="assistant-profile-tab" aria-controls="assistant-profile-panel" aria-selected={activeTab === 'assistant'} className={`inline-flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium ${activeTab === 'assistant' ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-500 hover:text-gray-800'}`} onClick={() => setActiveTab('assistant')}>
          <Bot size={16} />{language === 'en-US' ? 'Assistant profile' : '助手配置'}
        </button>
      </div>

      {activeTab === 'memory' ? (
        <section id="memory-panel" role="tabpanel" aria-labelledby="memory-tab" className="mt-6 max-w-3xl rounded-lg border border-amber-200 bg-amber-50 p-5">
          <h2 className="font-semibold text-amber-950">{language === 'en-US' ? 'Long-term memory is being redesigned' : '长期记忆正在重新设计'}</h2>
          <p className="mt-2 text-sm leading-6 text-amber-800">
            {language === 'en-US'
              ? 'The previous layered memory and background consolidation pipeline has been retired. Assistant conversations, Notebook actions, and Knowledge Base actions are not being captured as memory while the replacement design is under review.'
              : '原有的分层记忆与后台整理链路已经退役。在新方案完成评审前，Assistant 对话、Notebook 操作和知识库操作都不会被采集为长期记忆。'}
          </p>
        </section>
      ) : (
        <section id="assistant-profile-panel" role="tabpanel" aria-labelledby="assistant-profile-tab" className="mt-6 max-w-3xl rounded-lg border border-gray-200 bg-white p-5">
          <div className="flex items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-violet-50 text-violet-700"><Bot size={19} /></span>
            <div>
              <h2 className="font-semibold text-gray-950">{language === 'en-US' ? 'Assistant profile' : '助手配置'}</h2>
              <p className="mt-1 text-sm leading-6 text-gray-500">{language === 'en-US' ? 'Define the identity and behavior shared by new conversations.' : '定义所有新会话共用的助手身份与行为偏好。'}</p>
            </div>
          </div>
          <div className="mt-4 grid gap-4">
            <label className="grid gap-1.5 text-sm font-medium text-gray-800">
              {language === 'en-US' ? 'Assistant name' : '助手名称'}
              <input className="h-10 rounded-md border border-gray-300 bg-white px-3 text-sm font-normal outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100" value={assistantName} onChange={(event) => onAssistantProfileChange({ name: event.target.value, instructions: assistantInstructions })} />
            </label>
            <label className="grid gap-1.5 text-sm font-medium text-gray-800">
              {language === 'en-US' ? 'Behavior instructions' : '行为说明'}
              <textarea className="min-h-28 resize-y rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-normal leading-6 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100" value={assistantInstructions} onChange={(event) => onAssistantProfileChange({ name: assistantName, instructions: event.target.value })} />
            </label>
            <p className="text-xs text-gray-400">{language === 'en-US' ? 'Changes are saved automatically.' : '更改会自动保存。'}</p>
          </div>
        </section>
      )}
    </section>
  )
}
