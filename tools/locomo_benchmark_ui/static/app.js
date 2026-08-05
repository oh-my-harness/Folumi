const elements = {
  form: document.querySelector('#runForm'),
  kind: document.querySelector('#kind'),
  profile: document.querySelector('#profile'),
  dataset: document.querySelector('#dataset'),
  maxSamples: document.querySelector('#maxSamples'),
  maxQuestions: document.querySelector('#maxQuestions'),
  runId: document.querySelector('#runId'),
  answerSettings: document.querySelector('#answerSettings'),
  provider: document.querySelector('#provider'),
  model: document.querySelector('#model'),
  apiKey: document.querySelector('#apiKey'),
  baseUrl: document.querySelector('#baseUrl'),
  chatPath: document.querySelector('#chatPath'),
  assistantProfileMode: document.querySelector('#assistantProfileMode'),
  productProfileSummary: document.querySelector('#productProfileSummary'),
  customAssistantProfile: document.querySelector('#customAssistantProfile'),
  assistantName: document.querySelector('#assistantName'),
  assistantInstructions: document.querySelector('#assistantInstructions'),
  includeText: document.querySelector('#includeText'),
  keyHint: document.querySelector('#keyHint'),
  formError: document.querySelector('#formError'),
  startButton: document.querySelector('#startButton'),
  stopButton: document.querySelector('#stopButton'),
  statusPill: document.querySelector('#statusPill'),
  statusText: document.querySelector('#statusText'),
  runMeta: document.querySelector('#runMeta'),
  logOutput: document.querySelector('#logOutput'),
  resultKind: document.querySelector('#resultKind'),
  refreshResults: document.querySelector('#refreshResults'),
  resultWarnings: document.querySelector('#resultWarnings'),
  chartFrame: document.querySelector('#chartFrame'),
  chartImage: document.querySelector('#chartImage'),
  resultList: document.querySelector('#resultList'),
}

let state = null
let results = { runs: [], charts: {}, warnings: [] }
let previousPhase = 'idle'
let logSignature = ''

const phaseLabels = {
  idle: '空闲',
  starting: '正在启动',
  running: '运行中',
  stopping: '正在停止',
  succeeded: '已完成',
  failed: '失败',
  stopped: '已停止',
}

async function getJson(url) {
  const response = await fetch(url, { cache: 'no-store' })
  if (!response.ok) throw new Error(`请求失败：${response.status}`)
  return response.json()
}

async function postJson(url, payload = {}) {
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Benchmark-Token': state.token,
    },
    body: JSON.stringify(payload),
  })
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(data.error || `请求失败：${response.status}`)
  return data
}

function loadPreferences() {
  let stored = {}
  try {
    stored = JSON.parse(localStorage.getItem('folumi-locomo-preferences') || '{}')
  } catch {
    localStorage.removeItem('folumi-locomo-preferences')
  }
  elements.dataset.value = state.default_dataset || stored.dataset || ''
  elements.kind.value = stored.kind || 'retrieval'
  elements.profile.value = stored.profile || 'debug'
  elements.maxSamples.value = stored.maxSamples || ''
  elements.maxQuestions.value = stored.maxQuestions || ''
  elements.provider.value = stored.provider || 'anthropic'
  elements.model.value = stored.model || ''
  elements.baseUrl.value = stored.baseUrl || ''
  elements.chatPath.value = stored.chatPath || ''
  elements.assistantProfileMode.value = stored.assistantProfileMode || 'product'
  elements.assistantName.value = stored.assistantName || ''
  elements.assistantInstructions.value = stored.assistantInstructions || ''
  elements.resultKind.value = stored.resultKind || 'retrieval'
  updateMode()
}

function savePreferences() {
  localStorage.setItem('folumi-locomo-preferences', JSON.stringify({
    dataset: elements.dataset.value,
    kind: elements.kind.value,
    profile: elements.profile.value,
    maxSamples: elements.maxSamples.value,
    maxQuestions: elements.maxQuestions.value,
    provider: elements.provider.value,
    model: elements.model.value,
    baseUrl: elements.baseUrl.value,
    chatPath: elements.chatPath.value,
    assistantProfileMode: elements.assistantProfileMode.value,
    assistantName: elements.assistantName.value,
    assistantInstructions: elements.assistantInstructions.value,
    resultKind: elements.resultKind.value,
  }))
}

function updateMode() {
  const answer = elements.kind.value === 'answer'
  elements.answerSettings.hidden = !answer
  elements.model.required = answer
  updateKeyHint()
  updateAssistantProfileMode()
}

function updateAssistantProfileMode() {
  const productProfile = state?.product_assistant_profile
  const productOption = elements.assistantProfileMode.querySelector('option[value="product"]')
  productOption.disabled = !productProfile
  if (!productProfile && elements.assistantProfileMode.value === 'product') {
    elements.assistantProfileMode.value = 'custom'
  }
  const useProductProfile = elements.assistantProfileMode.value === 'product'
  elements.productProfileSummary.hidden = !useProductProfile
  elements.customAssistantProfile.hidden = useProductProfile
  if (productProfile) {
    const name = productProfile.name || 'Folumi Assistant'
    const instructions = productProfile.has_custom_instructions
      ? `已配置 ${productProfile.instructions_length} 个字符的身份说明`
      : '未填写说明，将使用 Folumi 默认身份说明'
    elements.productProfileSummary.textContent = `产品配置：${name}；${instructions}。`
  } else {
    elements.productProfileSummary.textContent = '未找到桌面端产品设置，请使用 Benchmark 专用配置。'
  }
}

function updateKeyHint() {
  const available = Boolean(state?.provider_keys?.[elements.provider.value])
  elements.keyHint.textContent = available
    ? '当前进程环境已检测到该服务商的 Key，可以留空。'
    : '当前环境未检测到该服务商的 Key，请在上方临时填写。'
}

function numberOrNull(input) {
  return input.value ? Number(input.value) : null
}

function showFormError(message = '') {
  elements.formError.textContent = message
  elements.formError.hidden = !message
}

async function startRun(event) {
  event.preventDefault()
  showFormError()
  if (!elements.form.reportValidity()) return
  const payload = {
    kind: elements.kind.value,
    profile: elements.profile.value,
    dataset: elements.dataset.value,
    max_samples: numberOrNull(elements.maxSamples),
    max_questions: numberOrNull(elements.maxQuestions),
    run_id: elements.runId.value,
    provider: elements.provider.value,
    model: elements.model.value,
    api_key: elements.apiKey.value,
    base_url: elements.baseUrl.value,
    chat_path: elements.chatPath.value,
    assistant_profile_mode: elements.assistantProfileMode.value,
    assistant_name: elements.assistantName.value,
    assistant_instructions: elements.assistantInstructions.value,
    include_text: elements.includeText.checked,
  }
  try {
    savePreferences()
    await postJson('/api/run', payload)
    elements.apiKey.value = ''
    await refreshStatus()
  } catch (error) {
    showFormError(error.message)
  }
}

async function stopRun() {
  try {
    await postJson('/api/stop')
    await refreshStatus()
  } catch (error) {
    showFormError(error.message)
  }
}

function updateStatusView(status) {
  const phase = status.phase || 'idle'
  const active = ['starting', 'running', 'stopping'].includes(phase)
  elements.statusPill.dataset.phase = phase
  elements.statusText.textContent = phaseLabels[phase] || phase
  elements.startButton.disabled = active
  elements.stopButton.disabled = !['starting', 'running'].includes(phase)
  elements.runMeta.textContent = status.run_id
    ? `${status.kind === 'answer' ? '回答' : '检索'} · ${status.run_id}${status.exit_code == null ? '' : ` · exit ${status.exit_code}`}`
    : '尚未运行'
  const logs = status.logs?.length ? status.logs.join('\n') : '等待开始评测……'
  if (logs !== logSignature) {
    const nearBottom = elements.logOutput.scrollHeight - elements.logOutput.scrollTop - elements.logOutput.clientHeight < 70
    elements.logOutput.textContent = logs
    if (nearBottom) elements.logOutput.scrollTop = elements.logOutput.scrollHeight
    logSignature = logs
  }
  if (previousPhase !== phase && ['succeeded', 'failed', 'stopped'].includes(phase)) {
    refreshResults()
  }
  previousPhase = phase
}

async function refreshStatus() {
  try {
    updateStatusView(await getJson('/api/status'))
  } catch (error) {
    elements.statusText.textContent = '连接中断'
    elements.statusPill.dataset.phase = 'failed'
  }
}

function percent(value) {
  return value == null ? 'N/A' : `${(Number(value) * 100).toFixed(1)}%`
}

function metric(label, value) {
  return `<div class="metric"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong></div>`
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;')
}

function resultMetrics(run) {
  const overall = run.overall || {}
  if (run.kind === 'retrieval') {
    const limit = run.configuration?.search_limit || 3
    return [
      ['Hit@1', percent(overall.hit_at_1)],
      [`Hit@${limit}`, percent(overall.hit_at_k)],
      [`MRR@${limit}`, percent(overall.mrr_at_k)],
      [`Evidence Recall@${limit}`, percent(overall.evidence_recall_at_k)],
    ]
  }
  return [
    ['回答分数', percent(overall.answer_f1)],
    ['Exact Match', percent(overall.exact_match)],
    ['Category 5 拒答', percent(overall.abstention_accuracy)],
    ['历史搜索率', percent(overall.search_rate)],
  ]
}

function categoryChips(run) {
  return Object.entries(run.categories || {})
    .sort(([left], [right]) => Number(left) - Number(right))
    .map(([category, metrics]) => {
      const value = run.kind === 'retrieval' ? metrics.hit_at_k : metrics.answer_f1
      return `<span class="category-chip">Category ${escapeHtml(category)} · ${percent(value)}</span>`
    })
    .join('')
}

function renderResults() {
  const kind = elements.resultKind.value
  const filtered = results.runs.filter((run) => run.kind === kind)
  const chart = results.charts?.[kind]
  elements.chartFrame.hidden = !chart
  if (chart) elements.chartImage.src = chart
  if (!filtered.length) {
    elements.resultList.innerHTML = '<div class="empty-state">还没有这一类型的结果。运行一次 Benchmark 后会显示在这里。</div>'
    return
  }
  elements.resultList.innerHTML = filtered.slice(0, 12).map((run) => {
    const metrics = resultMetrics(run).map(([label, value]) => metric(label, value)).join('')
    const questionCount = run.dataset_counts?.questions_scored ?? run.overall?.questions ?? '—'
    const model = run.configuration?.model ? ` · ${escapeHtml(run.configuration.model)}` : ''
    const assistant = run.configuration?.assistant_profile?.name
      ? ` · ${escapeHtml(run.configuration.assistant_profile.name)}`
      : ''
    return `
      <article class="result-card">
        <div class="result-card-header">
          <div>
            <h3>${escapeHtml(run.run_id || run.filename)}</h3>
            <div class="result-card-meta">${escapeHtml(run.generated_at || '未知时间')} · ${escapeHtml(run.profile || '未知模式')} · ${escapeHtml(questionCount)} 题${model}${assistant}</div>
          </div>
          <a href="${encodeURI(run.download_url)}" download>下载 JSON</a>
        </div>
        <div class="metric-grid">${metrics}</div>
        <div class="category-row">${categoryChips(run)}</div>
      </article>`
  }).join('')
}

async function refreshResults() {
  try {
    results = await getJson('/api/results')
    const warnings = results.warnings || []
    elements.resultWarnings.hidden = !warnings.length
    elements.resultWarnings.textContent = warnings.join('\n')
    renderResults()
  } catch (error) {
    elements.resultList.innerHTML = `<div class="empty-state">读取结果失败：${escapeHtml(error.message)}</div>`
  }
}

async function initialize() {
  try {
    state = await getJson('/api/state')
    loadPreferences()
    await Promise.all([refreshStatus(), refreshResults()])
    window.setInterval(refreshStatus, 1000)
  } catch (error) {
    showFormError(`控制台初始化失败：${error.message}`)
  }
}

elements.kind.addEventListener('change', () => { updateMode(); savePreferences() })
elements.provider.addEventListener('change', () => { updateKeyHint(); savePreferences() })
elements.assistantProfileMode.addEventListener('change', () => { updateAssistantProfileMode(); savePreferences() })
elements.resultKind.addEventListener('change', () => { savePreferences(); renderResults() })
elements.refreshResults.addEventListener('click', refreshResults)
elements.form.addEventListener('submit', startRun)
elements.stopButton.addEventListener('click', stopRun)

initialize()
