import { useCallback, useEffect, useMemo, useState } from 'react'
import { Archive, Download, RefreshCw } from 'lucide-react'

interface LegacyContinuityEntry {
  id: string
  tutor_id: string
  kind: string
  text: string
  next_action?: string | null
}

export function LegacyMigrationPanel({ language }: { language: 'zh-CN' | 'en-US' }) {
  const [entries, setEntries] = useState<LegacyContinuityEntry[]>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [exportAvailable, setExportAvailable] = useState(false)
  const [loading, setLoading] = useState(true)
  const [status, setStatus] = useState('')
  const keyFor = (entry: LegacyContinuityEntry) => `${entry.tutor_id}:${entry.id}`

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const response = await fetch('/api/migration/legacy')
      const data = await response.json().catch(() => ({})) as {
        continuity?: LegacyContinuityEntry[]
        quiz_export_available?: boolean
        tutor_export_available?: boolean
        error?: string
      }
      if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`)
      const continuity = data.continuity ?? []
      setEntries(continuity)
      setSelected(new Set(continuity.map(keyFor)))
      setExportAvailable(Boolean(data.quiz_export_available || data.tutor_export_available))
      setStatus('')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void load() }, [load])

  const selectedCount = useMemo(() => selected.size, [selected])

  const importSelected = async () => {
    if (!selectedCount) return
    setLoading(true)
    try {
      const response = await fetch('/api/migration/legacy/continuity', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ entry_ids: [...selected] }),
      })
      const data = await response.json().catch(() => ({})) as { count?: number; error?: string }
      if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`)
      setStatus(language === 'en-US'
        ? `Imported ${data.count ?? 0} item(s) into Assistant Continuity.`
        : `已将 ${data.count ?? 0} 条内容迁入助手连续性。`)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }

  const exportLegacy = async () => {
    setLoading(true)
    try {
      const response = await fetch('/api/migration/legacy/export.zip')
      if (!response.ok) {
        const data = await response.json().catch(() => ({})) as { error?: string }
        throw new Error(data.error || `HTTP ${response.status}`)
      }
      const url = URL.createObjectURL(await response.blob())
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = 'folumi-legacy-export.zip'
      anchor.click()
      URL.revokeObjectURL(url)
      setStatus(language === 'en-US' ? 'Legacy data exported.' : '旧数据已导出。')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLoading(false)
    }
  }

  if (!loading && entries.length === 0 && !exportAvailable) return null

  return (
    <section className="rounded-lg border border-amber-200 bg-amber-50/50 p-5">
      <div className="flex items-start gap-3">
        <Archive className="mt-0.5 text-amber-700" size={20} />
        <div className="min-w-0 flex-1">
          <h3 className="font-semibold text-gray-950">{language === 'en-US' ? 'Legacy data migration' : '旧数据迁移'}</h3>
          <p className="mt-1 text-sm leading-6 text-gray-600">{language === 'en-US'
            ? 'Export Quiz and Tutor data before removal. Only the continuity items you select are copied into the single Assistant profile.'
            : '在移除测验和多导师功能前导出旧数据；只有你选中的连续性内容会复制到单一助手中。'}</p>
        </div>
        <button type="button" className="rounded p-2 text-gray-500 hover:bg-white" disabled={loading} onClick={() => void load()} aria-label="Refresh legacy data"><RefreshCw size={16} className={loading ? 'animate-spin' : ''} /></button>
      </div>

      {entries.length > 0 && <div className="mt-4 space-y-2">
        {entries.map((entry) => {
          const key = keyFor(entry)
          return <label key={key} className="flex gap-3 rounded-lg border border-amber-100 bg-white px-3 py-3">
            <input type="checkbox" className="mt-1" checked={selected.has(key)} onChange={(event) => setSelected((current) => {
              const next = new Set(current)
              if (event.target.checked) next.add(key); else next.delete(key)
              return next
            })} />
            <span className="min-w-0"><span className="text-xs text-gray-500">{entry.kind.replaceAll('_', ' ')} · {entry.tutor_id}</span><span className="mt-1 block text-sm text-gray-800">{entry.text}</span>{entry.next_action && <span className="mt-1 block text-xs text-gray-500">{language === 'en-US' ? 'Next' : '下一步'}: {entry.next_action}</span>}</span>
          </label>
        })}
      </div>}

      <div className="mt-4 flex flex-wrap gap-2">
        {entries.length > 0 && <button type="button" className="rounded-lg bg-amber-700 px-3 py-2 text-sm font-medium text-white disabled:opacity-50" disabled={loading || !selectedCount} onClick={() => void importSelected()}>{language === 'en-US' ? `Import selected (${selectedCount})` : `迁入所选项（${selectedCount}）`}</button>}
        {exportAvailable && <button type="button" className="inline-flex items-center gap-2 rounded-lg border border-amber-200 bg-white px-3 py-2 text-sm font-medium text-amber-900" disabled={loading} onClick={() => void exportLegacy()}><Download size={15} />{language === 'en-US' ? 'Export all legacy data' : '导出全部旧数据'}</button>}
      </div>
      {status && <p className="mt-3 text-sm text-gray-700">{status}</p>}
    </section>
  )
}
