import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const sidebar = readFileSync(new URL('./components/Sidebar.tsx', import.meta.url), 'utf8')
const composer = readFileSync(new URL('./components/ChatBox.tsx', import.meta.url), 'utf8')
const onboarding = readFileSync(new URL('./components/OnboardingDialog.tsx', import.meta.url), 'utf8')
const migration = readFileSync(new URL('./components/LegacyMigrationPanel.tsx', import.meta.url), 'utf8')
const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8')
const knowledge = readFileSync(new URL('./components/KnowledgeBasePage.tsx', import.meta.url), 'utf8')
const settings = readFileSync(new URL('./components/SettingsPage.tsx', import.meta.url), 'utf8')
const memory = readFileSync(new URL('./components/UserMemoryPage.tsx', import.meta.url), 'utf8')

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
  assert.match(memory, /LegacyMigrationPanel/)
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

test('legacy teaching data is retired through explicit import or export', () => {
  assert.match(migration, /\/api\/migration\/legacy\/continuity/)
  assert.match(migration, /\/api\/migration\/legacy\/export\.zip/)
  assert.match(migration, /selected/)
})
