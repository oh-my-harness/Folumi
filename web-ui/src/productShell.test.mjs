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
  assert.match(memory, /id="memory-tab"/)
  assert.match(memory, /id="assistant-profile-tab"/)
  assert.doesNotMatch(settings, /Assistant profile|助手配置|assistantName|assistantInstructions/)
  assert.match(app, /onAssistantProfileChange/)
})

test('legacy data migration stays outside the active product boundary', () => {
  assert.doesNotMatch(backendMain, /migration_router|\/api\/migration\/legacy/)
  assert.doesNotMatch(backendRoutes, /pub mod migration/)
})

test('retired layered memory has no active capture or consolidation path', () => {
  const activeBackend = [backendMain, notebookRoutes, knowledgeRoutes, websocketRoutes, runtimeWorkflows].join('\n')
  assert.doesNotMatch(activeBackend, /MemoryEventCategory|record_event|memory_workflow|L1\/|L2\/|L3\//)
  assert.match(memory, /长期记忆正在重新设计|Long-term memory is being redesigned/)
  assert.doesNotMatch(memory, /\/api\/memory\/items|file_revision|marker/)
  assert.doesNotMatch(app, /approval_request|approval_response|ApprovalDialog/)
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
