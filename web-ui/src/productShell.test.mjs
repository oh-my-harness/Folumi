import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const sidebar = readFileSync(new URL('./components/Sidebar.tsx', import.meta.url), 'utf8')
const composer = readFileSync(new URL('./components/ChatBox.tsx', import.meta.url), 'utf8')
const onboarding = readFileSync(new URL('./components/OnboardingDialog.tsx', import.meta.url), 'utf8')
const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8')
const knowledge = readFileSync(new URL('./components/KnowledgeBasePage.tsx', import.meta.url), 'utf8')
const settings = readFileSync(new URL('./components/SettingsPage.tsx', import.meta.url), 'utf8')
const memory = readFileSync(new URL('./components/UserMemoryPage.tsx', import.meta.url), 'utf8')
const backendMain = readFileSync(new URL('../../crates/tutor-web/src/main.rs', import.meta.url), 'utf8')
const backendRoutes = readFileSync(new URL('../../crates/tutor-web/src/routes/mod.rs', import.meta.url), 'utf8')
const notebookRoutes = readFileSync(new URL('../../crates/tutor-web/src/routes/notebook.rs', import.meta.url), 'utf8')
const knowledgeRoutes = readFileSync(new URL('../../crates/tutor-web/src/routes/knowledge.rs', import.meta.url), 'utf8')
const websocketRoutes = readFileSync(new URL('../../crates/tutor-web/src/routes/ws.rs', import.meta.url), 'utf8')
const memoryStore = readFileSync(new URL('../../crates/tutor-web/src/memory_store.rs', import.meta.url), 'utf8')
const memoryRoutes = readFileSync(new URL('../../crates/tutor-web/src/routes/memory.rs', import.meta.url), 'utf8')
const runtimeWorkflows = readFileSync(new URL('../../crates/tutor-agent/src/runtime_workflow.rs', import.meta.url), 'utf8')

test('primary navigation exposes Assistant, Knowledge Base, Notebook, Memory, and Settings', () => {
  assert.match(sidebar, /export type AppView = 'assistant' \| 'knowledge' \| 'notebook' \| 'memory' \| 'settings'/)
  assert.match(sidebar, /key: 'assistant'/)
  assert.match(sidebar, /key: 'knowledge'/)
  assert.match(sidebar, /key: 'notebook'/)
  assert.match(sidebar, /key: 'memory'/)
  assert.doesNotMatch(sidebar, /key: '(tutor|space)'/)
})

test('Notebook and Memory are standalone workspaces while Knowledge Base stays RAG-only', () => {
  assert.match(app, /view === 'notebook'/)
  assert.match(app, /view === 'memory'/)
  assert.doesNotMatch(knowledge, /NotesPage|Search Sources and Notes/)
  assert.doesNotMatch(settings, /UserMemoryPage|LegacyMigrationPanel/)
  assert.doesNotMatch(memory, /LegacyMigrationPanel|\/api\/migration\/legacy/)
})

test('assistant profile is managed from Memory instead of Settings', () => {
  assert.match(memory, /Assistant profile|助手配置/)
  assert.match(memory, /assistantName/)
  assert.match(memory, /assistantInstructions/)
  assert.match(memory, /useState<'memory' \| 'assistant'>\('memory'\)/)
  assert.match(memory, /role="tablist"/)
  assert.match(memory, /id="memory"/)
  assert.match(memory, /id="assistant-profile"/)
  assert.match(memory, /id=\{`\$\{id\}-tab`\}/)
  assert.match(memory, /onSessionNavigate/)
  assert.doesNotMatch(settings, /Assistant profile|助手配置|assistantName|assistantInstructions/)
  assert.match(app, /onAssistantProfileChange/)
})

test('legacy data migration stays outside the active product boundary', () => {
  assert.doesNotMatch(backendMain, /migration_router|\/api\/migration\/legacy/)
  assert.doesNotMatch(backendRoutes, /pub mod migration/)
})

test('retired layered memory is replaced by explicit revisioned Saved Memory', () => {
  const activeBackend = [backendMain, notebookRoutes, knowledgeRoutes, websocketRoutes, runtimeWorkflows].join('\n')
  assert.doesNotMatch(activeBackend, /MemoryEventCategory|record_event|memory_workflow|L1\/|L2\/|L3\//)
  assert.match(memory, /Saved Memory|保存的记忆/)
  assert.match(memory, /\/api\/memory\/items/)
  assert.match(memory, /\/api\/memory\/export\.json/)
  assert.match(memory, /reconfirm: true/)
  assert.match(memoryRoutes, /revision/)
  assert.match(memoryStore, /memory-v2|memory_items|superseded|memory_tombstones/)
  assert.doesNotMatch(memory, /file_revision|marker|L1\/|L2\/|L3\//)
  assert.match(app, /approval_request|approval_response/)
  assert.match(backendRoutes, /pub mod memory/)
})

test('History Recall is visible tool search without hidden pre-run injection', () => {
  assert.match(memory, /不会在 run 前隐式自动召回/)
  assert.match(websocketRoutes, /source_id exactly `session_recall`/)
  assert.match(websocketRoutes, /tool trace and source link/)
  assert.doesNotMatch(websocketRoutes, /with_runtime_plugin|history_recall_plugin/)
})

test('research is a chat task action instead of a capability menu', () => {
  assert.doesNotMatch(composer, /openMenu === 'mode'/)
  assert.doesNotMatch(composer, /visibleModeOptions/)
  assert.match(composer, /aria-pressed=\{capability === 'research'\}/)
})

test('onboarding teaches model, knowledge, and asking without legacy hierarchy', () => {
  assert.match(onboarding, /steps: \['准备模型', '加入知识', '开始提问'\]/)
  assert.doesNotMatch(onboarding, /onManageTutors|onOpenMemory|OnboardingModeGuide/)
})

test('note references use the notes-only API and no Space picker contract', () => {
  assert.match(composer, /\/api\/notebook\/mentions/)
  assert.doesNotMatch(composer, /\/api\/space\/mentions|Space picker|spaceMention/)
})
