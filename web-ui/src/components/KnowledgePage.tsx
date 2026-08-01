import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import {
  AlertCircle,
  CheckCircle2,
  Database,
  FileText,
  Layers,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  RefreshCw,
  Search,
  Settings,
  Trash2,
  Upload,
  X,
} from 'lucide-react'
import { apiUrl } from '../api'
import { activeEmbeddingConfig, embeddingForSession } from '../settings'
import type { EmbeddingModelConfig, LlmSettings } from '../settings'
import type { SourceTarget } from './MarkdownMessage'

interface Props {
  settings: LlmSettings
  onChanged?: () => void
  focusTarget?: Extract<SourceTarget, { type: 'kb' }> | null
}

interface KnowledgeBaseItem {
  id: string
  name: string
  status: 'ready' | 'draft'
  embedding: {
    provider: string
    model: string
    base_url: string | null
    embeddings_path: string | null
    dimensions: number | null
    send_dimensions: boolean
    api_key_configured: boolean
  }
  documents: KnowledgeDocument[]
  created_at: string
  updated_at: string
}

interface KnowledgeDocument {
  id: string
  name: string
  source: string
  size_bytes: number
  chunks: number
  mime_type?: string | null
  file_path?: string | null
  created_at: string
}

interface SearchHit {
  id: string
  source: string
  text: string
  score?: number | null
}

interface SourceChunk {
  id: string
  text: string
}

interface IngestionJob {
  id: string
  status: 'queued' | 'running' | 'done' | 'error'
  stage: string
  message: string
  progress: number
  chunks?: number | null
  knowledge_base?: KnowledgeBaseItem | null
  error?: string | null
}

type TabKey = 'files' | 'upload' | 'indexes' | 'settings'
type UploadStatus = 'pending' | 'uploading' | 'processing' | 'done' | 'error'

interface UploadProgressItem {
  id: string
  name: string
  size: number
  progress: number
  status: UploadStatus
  message?: string
  error?: string
}

export function KnowledgePage({ settings, onChanged, focusTarget }: Props) {
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBaseItem[]>([])
  const [activeKbId, setActiveKbId] = useState<string | null>(null)
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null)
  const [focusedChunkId, setFocusedChunkId] = useState<string | null>(null)
  const [tab, setTab] = useState<TabKey>('files')
  const [kbSearch, setKbSearch] = useState('')
  const [source, setSource] = useState('pasted-text')
  const [text, setText] = useState('')
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<SearchHit[]>([])
  const [status, setStatus] = useState('所有更改已保存')
  const [busy, setBusy] = useState(false)
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [createError, setCreateError] = useState('')
  const [knowledgeListCollapsed, setKnowledgeListCollapsed] = useState(false)
  const [fileListCollapsed, setFileListCollapsed] = useState(false)
  const [newKbName, setNewKbName] = useState('')
  const [newKbEmbeddingId, setNewKbEmbeddingId] = useState(settings.activeEmbeddingConfigId ?? '')
  const [previewTextByDocId, setPreviewTextByDocId] = useState<Record<string, string>>({})
  const [chunksByDocId, setChunksByDocId] = useState<Record<string, SourceChunk[]>>({})
  const [chunkLoadingDocId, setChunkLoadingDocId] = useState<string | null>(null)
  const [selectedFiles, setSelectedFiles] = useState<File[]>([])
  const [uploadProgress, setUploadProgress] = useState<UploadProgressItem[]>([])
  const fileInputRef = useRef<HTMLInputElement>(null)

  const activeKb = knowledgeBases.find((item) => item.id === activeKbId) ?? knowledgeBases[0] ?? null
  const selectedDoc = activeKb?.documents.find((doc) => doc.id === selectedDocId) ?? null
  const selectedPreviewLoaded = selectedDoc
    ? Object.prototype.hasOwnProperty.call(previewTextByDocId, selectedDoc.id)
    : false
  const selectedPreviewText = selectedDoc && selectedPreviewLoaded ? previewTextByDocId[selectedDoc.id] : ''
  const activeEmbedding = activeEmbeddingConfig(settings)
  const selectedNewEmbedding =
    settings.embeddingConfigs.find((config) => config.id === newKbEmbeddingId) ?? activeEmbedding

  const openCreateDialog = () => {
    setNewKbName('')
    setNewKbEmbeddingId(settings.activeEmbeddingConfigId ?? settings.embeddingConfigs[0]?.id ?? '')
    setCreateError('')
    setCreateDialogOpen(true)
  }

  const closeCreateDialog = useCallback(() => {
    if (busy) return
    setCreateDialogOpen(false)
    setNewKbName('')
    setCreateError('')
  }, [busy])

  const filteredKnowledgeBases = useMemo(
    () =>
      knowledgeBases.filter((item) =>
        item.name.toLowerCase().includes(kbSearch.trim().toLowerCase()),
      ),
    [knowledgeBases, kbSearch],
  )

  useEffect(() => {
    void loadKnowledgeBases()
  }, [])

  useEffect(() => {
    const first = knowledgeBases[0]
    if (!activeKbId && first) setActiveKbId(first.id)
  }, [activeKbId, knowledgeBases])

  useEffect(() => {
    if (!activeKb || !selectedDoc || selectedPreviewLoaded) return
    void loadDocumentPreview(activeKb.id, selectedDoc.id)
  }, [activeKb, selectedDoc, selectedPreviewLoaded])

  const loadKnowledgeBases = async () => {
    try {
      const res = await fetch('/api/knowledge-bases')
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      setKnowledgeBases((data.knowledge_bases ?? []) as KnowledgeBaseItem[])
      setStatus('所有更改已保存')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    }
  }

  const loadDocumentPreview = async (kbId: string, documentId: string) => {
    try {
      const res = await fetch(
        `/api/knowledge-bases/${encodeURIComponent(kbId)}/documents/${encodeURIComponent(documentId)}/content`,
      )
      const data = await res.json()
      if (!res.ok) return
      setPreviewTextByDocId((prev) => ({ ...prev, [documentId]: data.text ?? '' }))
    } catch {
      // Preview is best-effort; indexing/search should not fail because of it.
    }
  }

  const applyUpdatedKnowledgeBase = (updated: KnowledgeBaseItem) => {
    setKnowledgeBases((prev) => prev.map((item) => (item.id === updated.id ? updated : item)))
    onChanged?.()
  }

  const createKnowledgeBase = async () => {
    if (!selectedNewEmbedding || busy) return
    setBusy(true)
    setCreateError('')
    setStatus('正在创建知识库...')
    try {
      const name = newKbName.trim() || `knowledge-${knowledgeBases.length + 1}`
      const res = await fetch('/api/knowledge-bases', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, embedding: embeddingForSession(selectedNewEmbedding) }),
      })
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      const created = data.knowledge_base as KnowledgeBaseItem
      setKnowledgeBases((prev) => [...prev, created])
      setActiveKbId(created.id)
      setSelectedDocId(null)
      setNewKbName('')
      setCreateError('')
      setCreateDialogOpen(false)
      setTab('upload')
      setStatus('知识库已创建')
      onChanged?.()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setCreateError(message)
      setStatus(message)
    } finally {
      setBusy(false)
    }
  }

  const deleteKnowledgeBase = async (id: string) => {
    if (busy) return
    setBusy(true)
    try {
      const res = await fetch(`/api/knowledge-bases/${encodeURIComponent(id)}`, { method: 'DELETE' })
      if (!res.ok && res.status !== 404) {
        const data = await safeJson(res)
        throw new Error(errorMessage(data, res.status))
      }
      setKnowledgeBases((prev) => prev.filter((item) => item.id !== id))
      if (activeKbId === id) {
        setActiveKbId(null)
        setSelectedDocId(null)
      }
      setStatus('知识库已删除')
      onChanged?.()
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const ingest = async () => {
    if (!activeKb || !text.trim() || busy) return
    setBusy(true)
    setStatus('正在创建入库任务...')
    try {
      const res = await fetch(`/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/documents`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source: source.trim() || 'pasted-text', text }),
      })
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      const job = await pollIngestionJob(data.job?.id, (next) => {
        setStatus(`${stageLabel(next.stage)} · ${next.message}`)
      })
      if (!job.knowledge_base) throw new Error('ingestion finished without knowledge base')
      const updated = job.knowledge_base
      const doc = updated.documents[0]
      applyUpdatedKnowledgeBase(updated)
      if (doc) {
        setSelectedDocId(doc.id)
        setPreviewTextByDocId((prev) => ({ ...prev, [doc.id]: text }))
      }
      setTab('files')
      setText('')
      setStatus(`已入库 ${job.chunks ?? 0} 个片段`)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const uploadFiles = async () => {
    if (!activeKb || selectedFiles.length === 0 || busy) return
    setBusy(true)
    const filesToUpload = selectedFiles.filter((file) => {
      const progress = uploadProgress.find((item) => item.id === uploadProgressId(file))
      return progress?.status !== 'done'
    })
    setStatus(`正在上传 ${filesToUpload.length} 个附件...`)
    if (uploadProgress.length === 0) setUploadProgress(selectedFiles.map(fileToProgressItem))
    try {
      let latestKb: KnowledgeBaseItem | null = null
      let latestDoc: KnowledgeDocument | null = null
      const previews: Record<string, string> = {}
      const failedFiles: File[] = []

      for (const file of filesToUpload) {
        const uploadId = uploadProgressId(file)
        let data: { knowledge_base: KnowledgeBaseItem; chunks?: number }
        try {
          data = await uploadKnowledgeFile(activeKb.id, file, (next) => {
            setUploadProgress((prev) =>
              prev.map((item) => (item.id === uploadId ? { ...item, ...next, error: undefined } : item)),
            )
          })
        } catch (err) {
          const message = uploadErrorMessage(err)
          setUploadProgress((prev) =>
            prev.map((item) =>
              item.id === uploadId
                ? { ...item, status: 'error', message: '上传失败', error: message }
                : item,
            ),
          )
          failedFiles.push(file)
          continue
        }
        latestKb = data.knowledge_base as KnowledgeBaseItem
        latestDoc = latestKb.documents[0] ?? null
        if (latestDoc && !isPdfFile(file)) {
          previews[latestDoc.id] = await file.text()
        }
        setUploadProgress((prev) =>
          prev.map((item) =>
            item.id === uploadId
              ? { ...item, progress: 100, status: 'done', message: `已入库 ${data.chunks ?? 0} 个片段` }
              : item,
          ),
        )
      }

      if (latestKb) {
        applyUpdatedKnowledgeBase(latestKb)
      }
      if (latestDoc) {
        setSelectedDocId(latestDoc.id)
        setPreviewTextByDocId((prev) => ({ ...prev, ...previews }))
      }
      if (failedFiles.length === 0) {
        setSelectedFiles([])
        if (fileInputRef.current) fileInputRef.current.value = ''
        setTab('files')
        setStatus('附件已入库')
      } else {
        setSelectedFiles(failedFiles)
        setStatus(`${filesToUpload.length - failedFiles.length} 个附件已入库，${failedFiles.length} 个失败，可直接重试`)
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setUploadProgress((prev) =>
        prev.map((item) =>
          item.status === 'done' || item.status === 'error'
            ? item
            : { ...item, status: 'error', message: '未上传', error: message },
        ),
      )
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const search = async () => {
    if (!activeKb || !query.trim() || busy) return
    setBusy(true)
    setStatus('正在检索...')
    try {
      const res = await fetch(`/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/search`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query, top_k: 5 }),
      })
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      setHits(data.hits ?? [])
      setStatus(`找到 ${(data.hits ?? []).length} 条结果`)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const deleteDocument = async (documentId: string) => {
    if (!activeKb || busy) return
    setBusy(true)
    try {
      const res = await fetch(
        `/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/documents/${encodeURIComponent(documentId)}`,
        { method: 'DELETE' },
      )
      const data = await safeJson(res)
      if (!res.ok) throw new Error(errorMessage(data, res.status))
      if (data.knowledge_base) {
        applyUpdatedKnowledgeBase(data.knowledge_base as unknown as KnowledgeBaseItem)
      } else {
        await loadKnowledgeBases()
      }
      setSelectedDocId((current) => (current === documentId ? null : current))
      setPreviewTextByDocId((prev) => omitKey(prev, documentId))
      setChunksByDocId((prev) => omitKey(prev, documentId))
      setStatus('文档已删除')
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const reindexDocument = async (documentId: string) => {
    if (!activeKb || busy) return
    setBusy(true)
    try {
      const res = await fetch(
        `/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/documents/${encodeURIComponent(documentId)}/reindex`,
        { method: 'POST' },
      )
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      const job = await pollIngestionJob(data.job?.id, (next) => {
        setStatus(`${stageLabel(next.stage)} · ${next.message}`)
      })
      if (job.knowledge_base) applyUpdatedKnowledgeBase(job.knowledge_base)
      setChunksByDocId((prev) => omitKey(prev, documentId))
      setStatus(`已重建 ${job.chunks ?? 0} 个片段`)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const reindexKnowledgeBase = async () => {
    if (!activeKb || busy) return
    setBusy(true)
    setStatus('正在从保存的原文重建全部索引...')
    try {
      const res = await fetch(`/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/reindex`, {
        method: 'POST',
      })
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      const job = await pollIngestionJob(data.job?.id, (next) => {
        setStatus(`${stageLabel(next.stage)} · ${next.message}`)
      })
      if (job.knowledge_base) applyUpdatedKnowledgeBase(job.knowledge_base)
      setChunksByDocId({})
      setStatus(`已从权威原文重建全部索引，共 ${job.chunks ?? 0} 个片段`)
    } catch (err) {
      setStatus(`整库重建失败：${err instanceof Error ? err.message : String(err)}`)
    } finally {
      setBusy(false)
    }
  }

  const loadDocumentChunks = async (documentId: string) => {
    if (!activeKb || busy) return
    setChunkLoadingDocId(documentId)
    try {
      const res = await fetch(
        `/api/knowledge-bases/${encodeURIComponent(activeKb.id)}/documents/${encodeURIComponent(documentId)}/chunks`,
      )
      const data = await res.json()
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`)
      setChunksByDocId((prev) => ({ ...prev, [documentId]: data.chunks ?? [] }))
      setStatus(`已加载 ${(data.chunks ?? []).length} 个片段`)
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err))
    } finally {
      setChunkLoadingDocId(null)
    }
  }

  useEffect(() => {
    if (!focusTarget || knowledgeBases.length === 0) return
    const kb = knowledgeBases.find((item) => item.id === focusTarget.knowledgeBaseId)
    if (!kb) {
      setStatus(`Knowledge base source not found: ${focusTarget.knowledgeBaseId}`)
      return
    }

    setActiveKbId(kb.id)
    setTab('files')
    setCreateDialogOpen(false)
    setFileListCollapsed(false)

    const doc = kb.documents.find((item) => item.id === focusTarget.documentId)
    if (!doc) {
      setSelectedDocId(kb.documents[0]?.id ?? null)
      setFocusedChunkId(null)
      setStatus(`Knowledge document source not found: ${focusTarget.documentId}`)
      return
    }

    setSelectedDocId(doc.id)
    setFocusedChunkId(focusTarget.chunkId ?? null)
    setStatus(focusTarget.chunkId ? `Opened knowledge source: ${doc.name} / ${focusTarget.chunkId}` : `Opened knowledge source: ${doc.name}`)
    if (focusTarget.chunkId && !chunksByDocId[doc.id]) {
      void loadDocumentChunks(doc.id)
    }
  }, [focusTarget, knowledgeBases, chunksByDocId])

  return (
    <main className="flex min-h-0 flex-1 bg-white">
      <KnowledgeListPanel
        collapsed={knowledgeListCollapsed}
        items={filteredKnowledgeBases}
        totalCount={knowledgeBases.length}
        activeId={activeKb?.id ?? null}
        search={kbSearch}
        canCreate={settings.embeddingConfigs.length > 0}
        busy={busy}
        onSearch={setKbSearch}
        onCollapse={() => setKnowledgeListCollapsed(true)}
        onExpand={() => setKnowledgeListCollapsed(false)}
        onCreate={openCreateDialog}
        onSelect={(item) => {
          setActiveKbId(item.id)
          setSelectedDocId(item.documents[0]?.id ?? null)
          setCreateDialogOpen(false)
        }}
        onDelete={deleteKnowledgeBase}
      />

      <section className="flex min-w-0 flex-1 flex-col">
        <header className="border-b border-gray-200 bg-white px-5 pt-4">
          <div className="flex flex-wrap items-start gap-3">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="truncate text-xl font-semibold text-gray-950">
                  {activeKb?.name ?? '知识库'}
                </h1>
                {activeKb && (
                  <Badge tone="green" icon={<CheckCircle2 size={14} />}>
                    {statusLabel(activeKb.status)}
                  </Badge>
                )}
              </div>
              <p className="mt-1.5 text-xs text-gray-600">
                {activeKb
                  ? `${activeKb.embedding.model} · ${activeKb.embedding.dimensions ?? '未知'}维 · 最近更新 ${formatTime(activeKb.updated_at)}`
                  : '创建知识库后，即可添加资料并进行语义检索。'}
              </p>
            </div>
            <div className="text-xs text-gray-500">{status}</div>
          </div>

          {activeKb && (
            <nav className="mt-4 flex gap-5">
              <TabButton active={tab === 'files'} icon={<FileText size={17} />} onClick={() => setTab('files')}>
                文件
              </TabButton>
              <TabButton active={tab === 'upload'} icon={<Upload size={17} />} onClick={() => setTab('upload')}>
                添加文档
              </TabButton>
              <TabButton active={tab === 'indexes'} icon={<Layers size={17} />} onClick={() => setTab('indexes')}>
                索引版本
              </TabButton>
              <TabButton active={tab === 'settings'} icon={<Settings size={17} />} onClick={() => setTab('settings')}>
                设置
              </TabButton>
            </nav>
          )}
        </header>

        {activeKb ? (
          <div className="flex min-h-0 flex-1">
            {tab === 'files' && (
              <FileListPanel
                activeKb={activeKb}
                selectedDocId={selectedDocId}
                collapsed={fileListCollapsed}
                onSelectDoc={setSelectedDocId}
                onUpload={() => setTab('upload')}
                onReload={loadKnowledgeBases}
                onToggleCollapsed={() => setFileListCollapsed((value) => !value)}
                onDeleteDoc={deleteDocument}
                onReindexDoc={reindexDocument}
              />
            )}
            <section className="min-w-0 flex-1 overflow-y-auto">
              {tab === 'upload' && (
                <Panel>
                  <div className="max-w-3xl">
                    <h3 className="text-lg font-semibold text-gray-950">添加文档</h3>
                    <p className="mt-1 text-sm text-gray-600">
                      将使用 {activeKb.embedding.model} 为文档生成向量。
                    </p>
                    <div className="mt-5 grid gap-4">
                      <div className="rounded-xl border border-dashed border-blue-200 bg-blue-50/40 p-4">
                        <input
                          ref={fileInputRef}
                          className="hidden"
                          type="file"
                          multiple
                          accept=".pdf,.txt,.md,.markdown,.csv,.json,.log,application/pdf,text/*,application/json"
                          onChange={(event) => {
                            const files = Array.from(event.target.files ?? [])
                            setSelectedFiles(files)
                            setUploadProgress(files.map(fileToProgressItem))
                          }}
                        />
                        <button
                          className="flex w-full items-center justify-center gap-2 rounded-lg bg-white px-4 py-5 text-sm font-medium text-blue-700 shadow-sm ring-1 ring-blue-100 hover:bg-blue-50 disabled:opacity-60"
                          type="button"
                          onClick={() => fileInputRef.current?.click()}
                          disabled={busy}
                        >
                          <Upload size={18} />
                          选择附件
                        </button>
                        <p className="mt-2 text-xs text-gray-500">
                          当前支持 PDF 和 UTF-8 文本附件，例如 txt、md、csv、json、log。
                        </p>
                        {(selectedFiles.length > 0 || uploadProgress.length > 0) && (
                          <div className="mt-3 rounded-lg border border-blue-100 bg-white p-3">
                            <div className="space-y-3">
                              {(uploadProgress.length > 0 ? uploadProgress : selectedFiles.map(fileToProgressItem)).map((file) => (
                                <UploadProgressRow key={file.id} item={file} />
                              ))}
                            </div>
                            <button
                              className={`${primaryButtonClassName} mt-3`}
                              type="button"
                              onClick={uploadFiles}
                              disabled={busy || selectedFiles.length === 0}
                            >
                              <Database size={17} />
                              {uploadProgress.some((item) => item.status === 'error') ? '重试失败项' : '上传并入库'}
                            </button>
                          </div>
                        )}
                      </div>
                      <Field label="文件名">
                        <input
                          className={inputClassName}
                          value={source}
                          onChange={(event) => setSource(event.target.value)}
                        />
                      </Field>
                      <Field label="内容">
                        <textarea
                          className={`${inputClassName} min-h-72 resize-none`}
                          value={text}
                          onChange={(event) => setText(event.target.value)}
                          placeholder="粘贴课程资料、讲义片段或笔记..."
                        />
                      </Field>
                      <button className={primaryButtonClassName} type="button" onClick={ingest} disabled={!text.trim() || busy}>
                        <Database size={17} />
                        写入索引
                      </button>
                    </div>
                  </div>
                </Panel>
              )}

              {tab === 'indexes' && (
                <Panel>
                  <div className="max-w-3xl">
                    <h3 className="text-lg font-semibold text-gray-950">索引版本</h3>
                    <p className="mt-1 text-sm text-gray-600">
                      当前版本绑定 {activeKb.embedding.model}。索引是可丢弃的派生数据，可随时从保存的原始文档重建。
                    </p>
                    <div className="mt-4 flex items-center gap-3 rounded-lg border border-blue-100 bg-blue-50/50 px-4 py-3">
                      <div className="min-w-0 flex-1 text-sm text-gray-600">
                        原始文档是权威数据；此操作会重建当前知识库全部向量片段，不会修改原文。
                      </div>
                      <button className={secondaryButtonClassName} type="button" onClick={reindexKnowledgeBase} disabled={busy || activeKb.documents.length === 0}>
                        <RefreshCw size={16} className={busy ? 'animate-spin' : ''} />
                        重建全部索引
                      </button>
                    </div>
                    <div className="mt-5 flex gap-3">
                      <input
                        className={inputClassName}
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                        placeholder="输入检索问题..."
                      />
                      <button className={primaryButtonClassName} type="button" onClick={search} disabled={!query.trim() || busy}>
                        <Search size={17} />
                        检索
                      </button>
                    </div>
                    <div className="mt-5 space-y-3">
                      {hits.map((hit) => (
                        <article key={hit.id} className="rounded-lg border border-gray-200 bg-gray-50 p-4">
                          <div className="mb-2 flex items-center justify-between text-xs text-gray-500">
                            <span className="truncate">{hit.source}</span>
                            <span>{typeof hit.score === 'number' ? hit.score.toFixed(4) : 'n/a'}</span>
                          </div>
                          <p className="text-sm leading-6 text-gray-800">{hit.text}</p>
                        </article>
                      ))}
                    </div>
                  </div>
                </Panel>
              )}

              {tab === 'settings' && (
                <Panel>
                  <div className="max-w-2xl">
                    <h3 className="text-lg font-semibold text-gray-950">设置</h3>
                    <dl className="mt-5 divide-y divide-gray-200 rounded-lg border border-gray-200">
                      <InfoRow label="嵌入模型" value={activeKb.embedding.model} />
                      <InfoRow label="Base URL" value={activeKb.embedding.base_url ?? '-'} />
                      <InfoRow label="端点" value={activeKb.embedding.embeddings_path ?? '-'} />
                      <InfoRow label="维度" value={String(activeKb.embedding.dimensions ?? '-')} />
                      <InfoRow label="文档数" value={String(activeKb.documents.length)} />
                    </dl>
                  </div>
                </Panel>
              )}

              {tab === 'files' && (
                <FilePreview
                  kbId={activeKb.id}
                  selectedDoc={selectedDoc}
                  previewText={selectedPreviewText}
                  chunks={selectedDoc ? chunksByDocId[selectedDoc.id] : undefined}
                  focusedChunkId={focusedChunkId}
                  chunksLoading={selectedDoc ? chunkLoadingDocId === selectedDoc.id : false}
                  onLoadChunks={selectedDoc ? () => loadDocumentChunks(selectedDoc.id) : undefined}
                />
              )}
            </section>
          </div>
        ) : (
          <EmptyKnowledgeState
            canCreate={settings.embeddingConfigs.length > 0}
            onCreate={openCreateDialog}
          />
        )}
      </section>

      {createDialogOpen && (
        <CreateKnowledgeBaseDialog
          settings={settings}
          name={newKbName}
          embeddingId={newKbEmbeddingId}
          busy={busy}
          error={createError}
          selectedEmbedding={selectedNewEmbedding}
          onNameChange={setNewKbName}
          onEmbeddingChange={setNewKbEmbeddingId}
          onClose={closeCreateDialog}
          onCreate={createKnowledgeBase}
        />
      )}
    </main>
  )
}

function KnowledgeListPanel({
  collapsed,
  items,
  totalCount,
  activeId,
  search,
  canCreate,
  busy,
  onSearch,
  onCollapse,
  onExpand,
  onCreate,
  onSelect,
  onDelete,
}: {
  collapsed: boolean
  items: KnowledgeBaseItem[]
  totalCount: number
  activeId: string | null
  search: string
  canCreate: boolean
  busy: boolean
  onSearch: (value: string) => void
  onCollapse: () => void
  onExpand: () => void
  onCreate: () => void
  onSelect: (item: KnowledgeBaseItem) => void
  onDelete: (id: string) => void
}) {
  if (collapsed) {
    return (
      <section className="flex w-12 shrink-0 flex-col items-center border-r border-gray-200 bg-gray-50 py-3">
        <button className={iconButtonClassName} type="button" title="展开知识库列表" onClick={onExpand}>
          <PanelLeftOpen size={17} />
        </button>
        <span className="mt-3 [writing-mode:vertical-rl] text-xs font-medium tracking-wide text-gray-500">
          知识库
        </span>
      </section>
    )
  }

  return (
    <section className="flex w-72 shrink-0 flex-col border-r border-gray-200 bg-gray-50">
      <div className="flex items-center justify-between px-4 py-3">
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold text-gray-950">知识库</h2>
          <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium text-blue-700">
            {totalCount}
          </span>
        </div>
        <button className={iconButtonClassName} type="button" title="收起" onClick={onCollapse}>
          <PanelLeftClose size={17} />
        </button>
      </div>

      <div className="px-4">
        <button
          className="flex h-10 w-full items-center justify-center gap-2 rounded-lg bg-blue-600 text-sm font-medium text-white shadow-sm shadow-blue-950/10 hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400"
          type="button"
          onClick={onCreate}
          disabled={!canCreate || busy}
        >
          <Plus size={17} />
          新建知识库
        </button>
        <div className="relative mt-3">
          <Search size={18} className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-500" />
          <input
            className="h-10 w-full rounded-lg border border-gray-200 bg-white pl-9 pr-3 text-sm outline-none placeholder:text-gray-400 focus:border-blue-300 focus:ring-2 focus:ring-blue-50"
            value={search}
            onChange={(event) => onSearch(event.target.value)}
            placeholder="搜索知识库..."
          />
        </div>
      </div>

      <div className="mt-2 flex-1 space-y-1.5 overflow-y-auto px-3 pb-4">
        {items.map((item) => (
          <button
            key={item.id}
            className={`group flex w-full items-start gap-2.5 rounded-lg border p-2.5 text-left ${
              item.id === activeId
                ? 'border-blue-100 bg-white shadow-sm shadow-blue-950/5'
                : 'border-transparent hover:bg-white'
            }`}
            type="button"
            onClick={() => onSelect(item)}
          >
            <span
              className={`mt-1 h-2.5 w-2.5 rounded-full ${
                item.status === 'ready' ? 'bg-emerald-500' : 'bg-gray-300'
              }`}
            />
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-2 font-medium text-gray-900">
                <Database size={16} className="shrink-0 text-gray-500" />
                <span className="truncate">{item.name}</span>
              </span>
              <span className="mt-1 block text-xs text-gray-500">
                {statusLabel(item.status)} · {item.documents.length} 个文档
              </span>
            </span>
            <span
              className="rounded p-1 text-gray-400 opacity-0 hover:bg-gray-100 hover:text-gray-700 group-hover:opacity-100"
              onClick={(event) => {
                event.stopPropagation()
                onDelete(item.id)
              }}
            >
              <Trash2 size={16} />
            </span>
          </button>
        ))}
      </div>
    </section>
  )
}

function EmptyKnowledgeState({
  canCreate,
  onCreate,
}: {
  canCreate: boolean
  onCreate: () => void
}) {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center px-6 py-12">
      <div className="max-w-sm text-center">
        <span className="mx-auto flex h-11 w-11 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
          <Database size={21} />
        </span>
        <h2 className="mt-4 text-base font-semibold text-gray-950">暂无知识库</h2>
        <p className="mt-2 text-sm leading-6 text-gray-500">
          {canCreate
            ? '新建知识库并选择嵌入模型，然后添加你的课程资料。'
            : '请先在设置中配置嵌入模型，再新建知识库。'}
        </p>
        <button
          className={`${primaryButtonClassName} mt-5`}
          type="button"
          onClick={onCreate}
          disabled={!canCreate}
        >
          <Plus size={17} />
          新建知识库
        </button>
      </div>
    </div>
  )
}

function CreateKnowledgeBaseDialog({
  settings,
  name,
  embeddingId,
  busy,
  error,
  selectedEmbedding,
  onNameChange,
  onEmbeddingChange,
  onClose,
  onCreate,
}: {
  settings: LlmSettings
  name: string
  embeddingId: string
  busy: boolean
  error: string
  selectedEmbedding: EmbeddingModelConfig | null
  onNameChange: (value: string) => void
  onEmbeddingChange: (value: string) => void
  onClose: () => void
  onCreate: () => void
}) {
  const nameInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    nameInputRef.current?.focus()
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) onClose()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [busy, onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-gray-950/30 px-4 py-6 backdrop-blur-[1px]"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <section
        className="w-full max-w-lg overflow-hidden rounded-lg border border-gray-200 bg-white shadow-2xl shadow-gray-950/15"
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-knowledge-base-title"
      >
        <header className="flex items-start justify-between gap-4 border-b border-gray-100 px-5 py-4">
          <div className="flex min-w-0 items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
              <Database size={18} />
            </span>
            <div className="min-w-0">
              <h2 id="create-knowledge-base-title" className="text-base font-semibold text-gray-950">
                新建知识库
              </h2>
              <p className="mt-1 text-xs leading-5 text-gray-500">
                嵌入模型创建后不可更换，后续入库和检索将固定使用该配置。
              </p>
            </div>
          </div>
          <button
            className={iconButtonClassName}
            type="button"
            title="关闭"
            aria-label="关闭新建知识库弹窗"
            onClick={onClose}
            disabled={busy}
          >
            <X size={18} />
          </button>
        </header>

        <form
          onSubmit={(event) => {
            event.preventDefault()
            onCreate()
          }}
        >
          <div className="grid gap-4 px-5 py-5">
            <Field label="知识库名称">
              <input
                ref={nameInputRef}
                className={inputClassName}
                value={name}
                onChange={(event) => onNameChange(event.target.value)}
                placeholder="例如：高等数学教材"
                disabled={busy}
              />
            </Field>
            <Field label="嵌入模型">
              <select
                className={inputClassName}
                value={embeddingId}
                onChange={(event) => onEmbeddingChange(event.target.value)}
                disabled={busy}
              >
                <option value="">选择嵌入模型</option>
                {settings.embeddingConfigs.map((config) => (
                  <option key={config.id} value={config.id}>
                    {config.name} · {config.model} · {config.dimensions}维
                  </option>
                ))}
              </select>
            </Field>
            {selectedEmbedding && (
              <div className="rounded-lg border border-blue-100 bg-blue-50 px-3 py-2.5 text-sm text-blue-800">
                创建后将固定使用 {selectedEmbedding.model}，维度 {selectedEmbedding.dimensions}。
              </div>
            )}
            {error && (
              <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-700" role="alert">
                <AlertCircle className="mt-0.5 shrink-0" size={16} />
                <span className="min-w-0 break-words">{error}</span>
              </div>
            )}
          </div>

          <footer className="flex items-center justify-end gap-2 border-t border-gray-100 bg-gray-50/60 px-5 py-3">
            <button
              className="inline-flex h-9 items-center justify-center rounded-lg px-4 text-sm font-medium text-gray-700 hover:bg-gray-100 disabled:opacity-50"
              type="button"
              onClick={onClose}
              disabled={busy}
            >
              取消
            </button>
            <button className={primaryButtonClassName} type="submit" disabled={!selectedEmbedding || busy}>
              <Plus size={17} />
              {busy ? '正在创建...' : '创建'}
            </button>
          </footer>
        </form>
      </section>
    </div>
  )
}

function FileListPanel({
  activeKb,
  selectedDocId,
  collapsed,
  onSelectDoc,
  onUpload,
  onReload,
  onToggleCollapsed,
  onDeleteDoc,
  onReindexDoc,
}: {
  activeKb: KnowledgeBaseItem
  selectedDocId: string | null
  collapsed: boolean
  onSelectDoc: (id: string | null) => void
  onUpload: () => void
  onReload: () => void
  onToggleCollapsed: () => void
  onDeleteDoc: (id: string) => void
  onReindexDoc: (id: string) => void
}) {
  if (collapsed) {
    return (
      <aside className="flex w-12 shrink-0 flex-col items-center border-r border-gray-200 bg-white py-3">
        <button className={iconButtonClassName} type="button" title="展开文件列表" onClick={onToggleCollapsed}>
          <PanelLeftOpen size={16} />
        </button>
        <span className="mt-3 [writing-mode:vertical-rl] text-xs font-medium tracking-wide text-gray-500">文件</span>
      </aside>
    )
  }

  return (
    <aside className="flex w-72 shrink-0 flex-col border-r border-gray-200 bg-white">
      <div className="flex h-12 items-center justify-between border-b border-gray-200 px-4">
        <div className="flex items-center gap-2">
          <span className="font-medium text-gray-900">文件</span>
          <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium text-blue-700">
            {activeKb.documents.length}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button className={iconButtonClassName} type="button" title="刷新" onClick={onReload}>
            <RefreshCw size={16} />
          </button>
          <button className={iconButtonClassName} type="button" title="收起" onClick={onToggleCollapsed}>
            <PanelLeftClose size={16} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3">
        {activeKb.documents.length === 0 ? (
          <button
            className="flex w-full items-center gap-2.5 rounded-lg border border-dashed border-gray-300 p-3 text-left text-sm text-gray-600 hover:bg-blue-50"
            type="button"
            onClick={onUpload}
          >
            <Upload size={18} />
            添加第一个文档
          </button>
        ) : (
          activeKb.documents.map((doc) => (
            <button
              key={doc.id}
              className={`group flex w-full items-start gap-2.5 rounded-lg p-2.5 text-left hover:bg-blue-50 ${
                doc.id === selectedDocId ? 'bg-blue-50 text-blue-900' : ''
              }`}
              type="button"
              onClick={() => onSelectDoc(doc.id)}
            >
              <FileText size={18} className="mt-0.5 shrink-0 text-gray-600" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium text-gray-900">{doc.name}</span>
                <span className="mt-1 block text-xs text-gray-500">
                  {formatSize(doc.size_bytes)} · {doc.chunks} chunks · {formatTime(doc.created_at)}
                </span>
              </span>
              <span className="flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
                <span
                  className="rounded p-1 text-gray-400 hover:bg-blue-100 hover:text-blue-700"
                  title="重建索引"
                  onClick={(event) => {
                    event.stopPropagation()
                    onReindexDoc(doc.id)
                  }}
                >
                  <RefreshCw size={15} />
                </span>
                <span
                  className="rounded p-1 text-gray-400 hover:bg-red-50 hover:text-red-600"
                  title="删除文档"
                  onClick={(event) => {
                    event.stopPropagation()
                    onDeleteDoc(doc.id)
                  }}
                >
                  <Trash2 size={15} />
                </span>
              </span>
            </button>
          ))
        )}
      </div>
    </aside>
  )
}

function FilePreview({
  kbId,
  selectedDoc,
  previewText,
  chunks,
  focusedChunkId,
  chunksLoading,
  onLoadChunks,
}: {
  kbId: string
  selectedDoc: KnowledgeDocument | null
  previewText?: string
  chunks?: SourceChunk[]
  focusedChunkId?: string | null
  chunksLoading?: boolean
  onLoadChunks?: () => void
}) {
  useEffect(() => {
    if (!focusedChunkId || !chunks) return
    window.setTimeout(() => {
      document.getElementById(`kb-chunk-${focusedChunkId}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    }, 0)
  }, [chunks, focusedChunkId])

  if (!selectedDoc) {
    return (
      <div className="flex h-full min-h-[460px] items-center justify-center px-8 text-center">
        <div>
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50 text-blue-700">
            <FileText size={24} />
          </div>
          <h3 className="mt-5 text-base font-semibold text-gray-950">请选择一个文件以预览</h3>
          <p className="mt-2 max-w-sm text-sm leading-6 text-gray-600">从左侧列表选择任意文档，可在此处直接预览。</p>
        </div>
      </div>
    )
  }

  return (
    <Panel>
      <div className="mx-auto max-w-5xl">
        <div className="mb-4 flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-50 text-blue-700">
            <FileText size={20} />
          </div>
          <div className="min-w-0">
            <h3 className="truncate text-lg font-semibold text-gray-950">{selectedDoc.name}</h3>
            <p className="mt-0.5 text-xs text-gray-500">
              {formatSize(selectedDoc.size_bytes)} · {selectedDoc.chunks} chunks · {formatTime(selectedDoc.created_at)}
            </p>
          </div>
          <button
            className="ml-auto inline-flex h-8 items-center gap-1.5 rounded-lg border border-gray-200 px-2.5 text-xs font-medium text-gray-600 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-60"
            type="button"
            onClick={onLoadChunks}
            disabled={!onLoadChunks || chunksLoading}
          >
            <Layers size={15} />
            {chunksLoading ? '加载中' : '查看片段'}
          </button>
        </div>
        {chunks && (
          <div className="mb-4 rounded-lg border border-blue-100 bg-blue-50/40 p-3">
            <div className="mb-2 text-xs font-medium text-blue-800">索引片段 · {chunks.length}</div>
            <div className="max-h-72 space-y-2 overflow-y-auto">
              {chunks.map((chunk, index) => (
                <article
                  key={chunk.id}
                  id={`kb-chunk-${chunk.id}`}
                  className={`scroll-mt-4 rounded-md border p-3 ${
                    focusedChunkId === chunk.id
                      ? 'border-blue-300 bg-blue-50 ring-2 ring-blue-100'
                      : 'border-blue-100 bg-white'
                  }`}
                >
                  <div className="mb-1 text-xs font-medium text-gray-500">Chunk {index + 1}</div>
                  <p className="text-sm leading-6 text-gray-700">{chunk.text}</p>
                </article>
              ))}
            </div>
          </div>
        )}
        {isPdfDocument(selectedDoc) && selectedDoc.file_path ? (
          <div className="overflow-hidden rounded-lg border border-gray-200 bg-gray-50">
            <iframe
              className="h-[72vh] w-full bg-white"
              src={apiUrl(`/api/knowledge-bases/${encodeURIComponent(kbId)}/documents/${encodeURIComponent(selectedDoc.id)}/file`)}
              title={selectedDoc.name}
            />
          </div>
        ) : previewText ? (
          <pre className="whitespace-pre-wrap rounded-lg border border-gray-200 bg-gray-50 p-4 font-sans text-sm leading-6 text-gray-800">
            {previewText}
          </pre>
        ) : (
          <div className="rounded-lg border border-gray-200 bg-gray-50 p-4 text-sm text-gray-600">
            该文档已入库。当前版本只保存文档元数据和向量索引，刷新后不保留原文预览。
          </div>
        )}
      </div>
    </Panel>
  )
}

function UploadProgressRow({ item }: { item: UploadProgressItem }) {
  const error = item.status === 'error' ? item.error || item.message : null
  return (
    <div className="rounded-lg border border-gray-100 bg-gray-50 p-3">
      <div className="flex items-center gap-2 text-sm text-gray-700">
        <FileText size={16} className="shrink-0 text-blue-600" />
        <span className="min-w-0 flex-1 truncate">{item.name}</span>
        <span className="text-xs text-gray-500">{formatSize(item.size)}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-gray-200">
        <div
          className={`h-full rounded-full transition-all ${
            item.status === 'error' ? 'bg-red-500' : item.status === 'done' ? 'bg-emerald-500' : 'bg-blue-600'
          }`}
          style={{ width: `${Math.max(4, item.progress)}%` }}
        />
      </div>
      <div className="mt-1.5 flex items-center justify-between text-xs">
        <span className={item.status === 'error' ? 'text-red-600' : 'text-gray-500'} title={item.message}>
          {item.message ?? uploadStatusLabel(item.status)}
        </span>
        <span className="tabular-nums text-gray-500">{Math.round(item.progress)}%</span>
      </div>
      {error && (
        <div className="mt-2 rounded-md bg-red-50 px-2.5 py-2 text-xs leading-5 text-red-700" title={error}>
          {error}
        </div>
      )}
    </div>
  )
}

function Badge({ tone, icon, children }: { tone: 'green'; icon: ReactNode; children: ReactNode }) {
  const toneClass = tone === 'green' ? 'bg-emerald-50 text-emerald-700 ring-1 ring-emerald-100' : ''
  return <span className={`inline-flex h-6 items-center gap-1 rounded-full px-2 text-xs font-medium ${toneClass}`}>{icon}{children}</span>
}

function TabButton({ active, icon, children, onClick }: { active: boolean; icon: ReactNode; children: ReactNode; onClick: () => void }) {
  return (
    <button
      className={`flex h-10 items-center gap-1.5 border-b-2 text-sm font-medium ${
        active ? 'border-blue-600 text-blue-700' : 'border-transparent text-gray-600 hover:text-gray-950'
      }`}
      type="button"
      onClick={onClick}
    >
      {icon}
      {children}
    </button>
  )
}

function Panel({ children }: { children: ReactNode }) {
  return <div className="p-5">{children}</div>
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium text-gray-700">{label}</span>
      {children}
    </label>
  )
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3 text-sm">
      <dt className="text-gray-500">{label}</dt>
      <dd className="truncate font-medium text-gray-900">{value}</dd>
    </div>
  )
}

function uploadKnowledgeFile(
  kbId: string,
  file: File,
  onProgress: (next: Partial<UploadProgressItem>) => void,
): Promise<{ knowledge_base: KnowledgeBaseItem; chunks?: number }> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()
    const form = new FormData()
    form.append('file', file)
    xhr.timeout = 120_000

    xhr.upload.onprogress = (event) => {
      if (!event.lengthComputable) {
        onProgress({ status: 'uploading', progress: 15, message: '正在上传...' })
        return
      }
      const progress = Math.min(35, Math.round((event.loaded / event.total) * 35))
      onProgress({
        status: progress >= 35 ? 'processing' : 'uploading',
        progress,
        message: progress >= 35 ? '等待后端处理...' : '正在上传...',
      })
    }

    xhr.onloadstart = () => {
      onProgress({ status: 'uploading', progress: 5, message: '正在上传...' })
    }

    xhr.onload = () => {
      let data: { job?: IngestionJob; error?: string } = {}
      try {
        data = JSON.parse(xhr.responseText || '{}')
      } catch {
        reject(new Error(`上传响应不是有效 JSON：${truncateText(xhr.responseText || 'empty response', 180)}`))
        return
      }

      if (xhr.status < 200 || xhr.status >= 300 || !data.job?.id) {
        reject(new Error(data.error || xhr.statusText || `HTTP ${xhr.status}`))
        return
      }

      pollIngestionJob(data.job.id, (job) => {
        onProgress({
          status: job.status === 'error' ? 'error' : job.status === 'done' ? 'done' : 'processing',
          progress: Math.max(35, job.progress),
          message: `${stageLabel(job.stage)} · ${job.message}`,
        })
      })
        .then((job) => {
          if (!job.knowledge_base) throw new Error('ingestion finished without knowledge base')
          resolve({ knowledge_base: job.knowledge_base, chunks: job.chunks ?? undefined })
        })
        .catch(reject)
    }

    xhr.onerror = () => {
      reject(new Error('上传请求失败：网络连接中断或后端服务未响应，请检查 tutor-web 是否仍在运行，并查看服务端日志。'))
    }
    xhr.ontimeout = () => reject(new Error('上传请求超时：后端在 120 秒内没有返回入库任务。'))
    xhr.onabort = () => reject(new Error('上传已取消。'))
    xhr.open('POST', `/api/knowledge-bases/${encodeURIComponent(kbId)}/documents/upload`)
    xhr.send(form)
  })
}

async function safeJson(res: Response): Promise<Record<string, unknown>> {
  try {
    return await res.json()
  } catch {
    return {}
  }
}

function errorMessage(data: Record<string, unknown>, status: number) {
  return typeof data.error === 'string' ? data.error : `HTTP ${status}`
}

function uploadErrorMessage(err: unknown) {
  const message = err instanceof Error ? err.message : String(err)
  return message.trim() || '上传失败：未返回错误详情'
}

function truncateText(text: string, maxLength: number) {
  const normalized = text.replace(/\s+/g, ' ').trim()
  return normalized.length > maxLength ? `${normalized.slice(0, maxLength)}...` : normalized
}

async function pollIngestionJob(
  jobId: string | undefined,
  onProgress?: (job: IngestionJob) => void,
): Promise<IngestionJob> {
  if (!jobId) throw new Error('ingestion job was not created')
  for (;;) {
    const res = await fetch(`/api/ingest-jobs/${encodeURIComponent(jobId)}`)
    const data = await safeJson(res)
    if (!res.ok) throw new Error(errorMessage(data, res.status))
    const job = data.job as IngestionJob
    onProgress?.(job)
    if (job.status === 'done') return job
    if (job.status === 'error') throw new Error(job.error || job.message || 'ingestion failed')
    await delay(500)
  }
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function stageLabel(stage: string) {
  const labels: Record<string, string> = {
    queued: '排队',
    parse: '解析',
    chunk: '分片',
    embed: '嵌入',
    index: '写入索引',
    store: '保存',
    delete: '清理',
    done: '完成',
    error: '失败',
  }
  return labels[stage] ?? stage
}

function omitKey<T>(value: Record<string, T>, key: string): Record<string, T> {
  const next = { ...value }
  delete next[key]
  return next
}

function statusLabel(status: KnowledgeBaseItem['status']) {
  return status === 'ready' ? '就绪' : '草稿'
}

function formatSize(value: number | string) {
  const bytes = typeof value === 'number' ? value : new Blob([value]).size
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function formatTime(value: string) {
  if (!value) return '暂无'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function isPdfFile(file: File) {
  return file.type === 'application/pdf' || file.name.toLowerCase().endsWith('.pdf')
}

function isPdfDocument(document: KnowledgeDocument) {
  return (
    document.mime_type === 'application/pdf' ||
    document.name.toLowerCase().endsWith('.pdf') ||
    document.source.toLowerCase().endsWith('.pdf')
  )
}

function fileToProgressItem(file: File): UploadProgressItem {
  return {
    id: uploadProgressId(file),
    name: file.name,
    size: file.size,
    progress: 0,
    status: 'pending',
    message: '等待上传',
  }
}

function uploadProgressId(file: File) {
  return `${file.name}-${file.size}-${file.lastModified}`
}

function uploadStatusLabel(status: UploadStatus) {
  if (status === 'uploading') return '正在上传...'
  if (status === 'processing') return '正在解析并写入索引...'
  if (status === 'done') return '已完成'
  if (status === 'error') return '处理失败'
  return '等待上传'
}

const iconButtonClassName =
  'inline-flex h-7 w-7 items-center justify-center rounded text-gray-500 hover:bg-blue-50 hover:text-blue-700'

const inputClassName =
  'w-full rounded-lg border border-gray-200 bg-white px-3 py-1.5 text-sm text-gray-900 outline-none placeholder:text-gray-400 focus:border-blue-300 focus:ring-2 focus:ring-blue-50 disabled:bg-gray-50'

const primaryButtonClassName =
  'inline-flex h-9 items-center justify-center gap-2 rounded-lg bg-blue-600 px-3.5 text-sm font-medium text-white shadow-sm shadow-blue-950/10 hover:bg-blue-700 disabled:bg-gray-200 disabled:text-gray-400'

const secondaryButtonClassName =
  'inline-flex h-9 shrink-0 items-center justify-center gap-2 rounded-lg border border-gray-200 bg-white px-3.5 text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50'
