import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./sessionResilience.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)
const {
  appendCompletedSessionMessage,
  completedStreamText,
  isCurrentSessionEvent,
  isLatestSessionHydration,
  isLatestSessionLoad,
  reconcileSessionMessages,
  reconcileSessionRunState,
} = module.exports

test('canonical final text replaces provisional streamed progress', () => {
  assert.equal(
    completedStreamText('I remembered it before calling the tool.', 'The preference was saved.'),
    'The preference was saved.',
  )
  assert.equal(completedStreamText('plain streamed answer', ''), 'plain streamed answer')
})

test('rejects events and session loads from a stale selection', () => {
  assert.equal(isCurrentSessionEvent('session-a', 'session-b'), false)
  assert.equal(isCurrentSessionEvent('session-b', 'session-b'), true)
  assert.equal(isLatestSessionLoad(1, 2, 'session-a', 'session-b'), false)
  assert.equal(isLatestSessionLoad(2, 2, 'session-b', 'session-b'), true)
})

test('rejects an older hydration response for the same selected session', () => {
  assert.equal(isLatestSessionHydration(2, 2, 4, 5, 'session-a', 'session-a'), false)
  assert.equal(isLatestSessionHydration(2, 2, 5, 5, 'session-a', 'session-a'), true)
})

test('reconciles run indicators without changing session order or titles', () => {
  const current = [
    { id: 'pinned', title: 'Pinned', activeRun: { status: 'running' } },
    { id: 'recent', title: 'Recent', activeRun: { status: 'running' } },
  ]
  const reconciled = reconcileSessionRunState(current, [
    { id: 'recent', activeRun: null },
    { id: 'pinned', activeRun: { status: 'running', current_stage: 'search' } },
  ])

  assert.deepEqual(reconciled.map((session) => session.id), ['pinned', 'recent'])
  assert.equal(reconciled[0].title, 'Pinned')
  assert.equal(reconciled[0].activeRun.current_stage, 'search')
  assert.equal(reconciled[1].activeRun, null)
})

test('session hydration preserves messages received while history was loading', () => {
  const restored = [
    { role: 'user', text: 'question' },
  ]
  const live = [
    { role: 'status', text: 'Working' },
    { role: 'assistant', text: 'answer completed during reconnect' },
  ]

  assert.deepEqual(reconcileSessionMessages(restored, live), [...restored, ...live])
})

test('session hydration prefers durable copies without collapsing repeated turns', () => {
  const restored = [
    { role: 'user', text: 'repeat' },
    { role: 'assistant', text: 'same answer', citations: [{ id: 1 }] },
    { role: 'user', text: 'repeat' },
    { role: 'assistant', text: 'same answer', citations: [{ id: 2 }] },
  ]
  const live = [
    { role: 'assistant', text: 'same answer' },
    { role: 'assistant', text: 'same answer' },
  ]

  assert.deepEqual(reconcileSessionMessages(restored, live), restored)
})

test('completed snapshot replay does not duplicate the latest assistant answer', () => {
  const messages = [
    { role: 'user', text: 'question' },
    { role: 'assistant', text: 'answer' },
    { role: 'status', text: 'Done' },
  ]

  assert.equal(
    appendCompletedSessionMessage(messages, { role: 'assistant', text: 'answer' }),
    messages,
  )
  assert.deepEqual(
    appendCompletedSessionMessage(messages, { role: 'assistant', text: 'new answer' }),
    [...messages, { role: 'assistant', text: 'new answer' }],
  )
})
