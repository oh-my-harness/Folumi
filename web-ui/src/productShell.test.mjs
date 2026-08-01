import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const sidebar = readFileSync(new URL('./components/Sidebar.tsx', import.meta.url), 'utf8')
const composer = readFileSync(new URL('./components/ChatBox.tsx', import.meta.url), 'utf8')
const onboarding = readFileSync(new URL('./components/OnboardingDialog.tsx', import.meta.url), 'utf8')
const migration = readFileSync(new URL('./components/LegacyMigrationPanel.tsx', import.meta.url), 'utf8')

test('primary navigation exposes only Assistant, Knowledge Base, and Settings', () => {
  assert.match(sidebar, /export type AppView = 'assistant' \| 'knowledge' \| 'settings'/)
  assert.match(sidebar, /key: 'assistant'/)
  assert.match(sidebar, /key: 'knowledge'/)
  assert.doesNotMatch(sidebar, /key: '(tutor|notebook|space|memory)'/)
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
