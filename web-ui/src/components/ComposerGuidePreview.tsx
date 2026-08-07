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
      description: '临时带上一份文件，只用于当前消息。',
      steps: ['点击“附件”并选择一个或多个支持的文本类文件。', '确认文件标签出现在输入框上方，再随问题一起发送。'],
    },
    source: {
      toolbar: '不关联资料',
      label: '选择聊天参考内容',
      description: '让整次聊天都能参考一个资料集或你的笔记。',
      steps: ['先准备好资料集或笔记。', '回到聊天，从这里选择需要的内容。'],
    },
    mention: {
      toolbar: '@ 笔记',
      label: '使用 @ 指定笔记',
      description: '直接告诉助手这条消息要看哪篇笔记。',
      steps: ['点击 @ 按钮并搜索笔记。', '选择目标后会出现引用标签，再输入具体要求并发送。'],
    },
    model: {
      toolbar: '选择模型',
      label: '选择对话模型',
      description: '选择这次聊天由哪个模型回答。',
      steps: ['需要不同速度、能力或成本时切换模型。', '若列表为空，先到“设置 > 对话模型”添加配置。'],
    },
    send: {
      toolbar: '发送',
      label: '发送与停止',
      description: '点击箭头发送；回答时可在同一位置停止。',
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
      description: 'Attach a file temporarily for this message only.',
      steps: ['Click Attachments and choose one or more supported text files.', 'Confirm their chips appear above the toolbar, then send them with your question.'],
    },
    source: {
      toolbar: 'No sources',
      label: 'Choose conversation references',
      description: 'Let the whole conversation reference one source collection or your notes.',
      steps: ['Prepare a source collection or notes first.', 'Return to Chat and choose what you need here.'],
    },
    mention: {
      toolbar: '@ Notes',
      label: 'Choose a note with @',
      description: 'Tell the Assistant exactly which note to read for this message.',
      steps: ['Click @ Notes and search.', 'Select a target, confirm its chip appears, and add your instruction.'],
    },
    model: {
      toolbar: 'Select model',
      label: 'Select a conversation model',
      description: 'Choose which model answers this conversation.',
      steps: ['Switch when you need different speed, capability, or cost.', 'If the list is empty, add a model under Settings > Chat models.'],
    },
    send: {
      toolbar: 'Send',
      label: 'Send and stop',
      description: 'Click the arrow to send. Use the same spot to stop an answer.',
      steps: ['Enter text or add attachments/@ references, then click the arrow.', 'Click the stop control in the same location if the run should end early.'],
    },
  },
}
