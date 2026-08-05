import { ArrowUp, AtSign, Brain, Database, Paperclip } from 'lucide-react'
import type { ReactNode } from 'react'
import { useI18n } from '../i18n'
import { composerGuideControls, type ComposerGuideControl } from '../productGuide'

interface Props {
  control: ComposerGuideControl
  onControlChange: (control: ComposerGuideControl) => void
  compact?: boolean
}

const controlIcons = {
  attachment: <Paperclip size={18} />,
  source: <Database size={18} />,
  mention: <AtSign size={18} />,
  model: <Brain size={16} />,
  send: <ArrowUp size={19} />,
} satisfies Record<ComposerGuideControl, ReactNode>

export function ComposerGuidePreview({ control, onControlChange, compact = false }: Props) {
  const { language } = useI18n()
  const copy = language === 'en-US' ? englishCopy : chineseCopy
  const detail = copy.controls[control]

  return (
    <div>
      <div className="rounded-3xl border border-blue-100 bg-white shadow-sm">
        <div className={`${compact ? 'min-h-16 px-5 py-4 text-sm' : 'min-h-20 px-5 py-4 text-sm'} text-gray-400`}>
          {copy.placeholder}
        </div>
        <div className="flex flex-wrap items-center gap-2 border-t border-blue-50 px-4 py-2">
          {composerGuideControls.map((item) => {
            const itemCopy = copy.controls[item]
            const isSend = item === 'send'
            return (
              <button
                key={item}
                type="button"
                className={`${isSend ? 'ml-auto h-9 w-9 justify-center bg-blue-600 px-0 text-white hover:bg-blue-700' : 'h-9 gap-2 px-3 text-gray-600 hover:bg-blue-50'} inline-flex items-center rounded-full transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                  control === item ? (isSend ? 'ring-2 ring-blue-300 ring-offset-2' : 'bg-blue-50 text-blue-700 ring-2 ring-blue-200') : ''
                }`}
                title={itemCopy.label}
                aria-pressed={control === item}
                onClick={() => onControlChange(item)}
              >
                {controlIcons[item]}
                {!isSend && <span className="text-sm">{itemCopy.toolbar}</span>}
              </button>
            )
          })}
        </div>
      </div>

      <div className={`${compact ? 'mt-3 px-1' : 'mt-4 border-l-2 border-blue-500 pl-4'}`} aria-live="polite">
        <div className="flex items-center gap-2 text-sm font-semibold text-gray-950">
          <span className="text-blue-600">{controlIcons[control]}</span>
          {detail.label}
        </div>
        <p className="mt-1 text-sm leading-5 text-gray-600">{detail.description}</p>
        {!compact && (
          <ol className="mt-2 space-y-1 text-sm leading-5 text-gray-600">
            {detail.steps.map((step, index) => (
              <li key={step} className="flex gap-2">
                <span className="w-4 shrink-0 font-medium text-blue-700">{index + 1}.</span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  )
}

const chineseCopy = {
  placeholder: '在这里输入问题；按 Enter 发送，Shift + Enter 换行',
  controls: {
    attachment: {
      toolbar: '附件',
      label: '上传临时资料',
      description: '回形针按钮。上传的文件只作为当前消息的临时上下文，不会自动进入知识库或 Notebook。',
      steps: ['点击“附件”并选择一个或多个支持的文本类文件。', '确认文件标签出现在输入框上方，再随问题一起发送。'],
    },
    source: {
      toolbar: '不关联知识库',
      label: '关联知识库或 Notebook',
      description: '数据库按钮。为整个会话关联一个资料源：知识库使用检索，Notebook 使用 Markdown 文本搜索；当前一次只能选择其中一种。',
      steps: ['先在知识库中完成嵌入配置和资料入库，或在 Notebook 中准备笔记。', '回到会话，从该下拉框选择具体知识库或 Notebook。'],
    },
    mention: {
      toolbar: '@ 笔记',
      label: '使用 @ 精确引用目标',
      description: '@ 按钮用于点名一条笔记，比关联整个资料源更精确。',
      steps: ['点击 @ 按钮并搜索笔记。', '选择目标后会出现引用标签，再输入具体要求并发送。'],
    },
    model: {
      toolbar: '选择模型',
      label: '选择对话模型',
      description: '输入框右侧的模型按钮。新会话可在已配置的模型服务之间切换。',
      steps: ['需要不同速度、能力或成本时切换模型。', '若列表为空，先到“设置 > LLM”添加并测试配置。'],
    },
    send: {
      toolbar: '发送',
      label: '发送与停止',
      description: '蓝色箭头发送消息。Agent 运行后，同一位置会变为停止按钮，可中止当前生成。',
      steps: ['输入文字，或添加附件/@ 引用后点击箭头。', '生成过程中需要中止时，点击同一位置的停止按钮。'],
    },
  },
}

const englishCopy: typeof chineseCopy = {
  placeholder: 'Type a question here; Enter sends and Shift + Enter adds a line',
  controls: {
    attachment: {
      toolbar: 'Attachments',
      label: 'Upload temporary material',
      description: 'The paperclip button. Uploaded files are temporary context for the current message and are not added to Knowledge Base or Notebook.',
      steps: ['Click Attachments and choose one or more supported text files.', 'Confirm their chips appear above the toolbar, then send them with your question.'],
    },
    source: {
      toolbar: 'No knowledge base',
      label: 'Associate Knowledge Base or Notebook',
      description: 'The database button associates one source with the conversation. Knowledge Base uses retrieval; Notebook uses Markdown text search. Only one can be selected at a time.',
      steps: ['First ingest material into a Knowledge Base or prepare notes in Notebook.', 'Return to Chat and select the Knowledge Base or Notebook from this menu.'],
    },
    mention: {
      toolbar: '@ Notes',
      label: 'Reference an exact target with @',
      description: 'The @ button names one note and is more precise than associating an entire source.',
      steps: ['Click @ Notes and search.', 'Select a target, confirm its chip appears, and add your instruction.'],
    },
    model: {
      toolbar: 'Select model',
      label: 'Select a conversation model',
      description: 'The model button on the right switches among configured services for a new conversation.',
      steps: ['Switch when you need different speed, capability, or cost.', 'If the list is empty, add and test a profile under Settings > LLM.'],
    },
    send: {
      toolbar: 'Send',
      label: 'Send and stop',
      description: 'The blue arrow sends. While the Agent is running, the same position becomes a stop button for the current generation.',
      steps: ['Enter text or add attachments/@ references, then click the arrow.', 'Click the stop control in the same location if the run should end early.'],
    },
  },
}
