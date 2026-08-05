interface Props {
  spent: number
  limit: number
  warning: boolean
  language: 'zh-CN' | 'en-US'
}

export function BudgetPanel({ spent, limit, warning, language }: Props) {
  const pct = limit > 0 ? Math.min((spent / limit) * 100, 100) : 0
  const english = language === 'en-US'
  return (
    <div className="w-52" title={english ? 'Current session spend and limit' : '当前会话已用费用与上限'}>
      <div className="mb-1.5 flex items-center justify-between gap-4 text-xs">
        <span className="text-gray-500">{english ? 'Session cost' : '会话费用'}</span>
        <span className={warning ? 'font-medium text-amber-700' : 'font-medium text-gray-700'}>
          ${spent.toFixed(4)} / ${limit.toFixed(2)}
        </span>
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-gray-200">
        <div
          className={`h-full rounded-full transition-all ${pct > 80 ? 'bg-amber-400' : 'bg-blue-500'}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  )
}
