import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./productGuide.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)

const { guideAssistantStarterPrompt, normalizeProductGuideState } = module.exports

test('restores valid independent help navigation state', () => {
  assert.deepEqual(normalizeProductGuideState({ topic: 'notebook', composerControl: 'source' }), {
    topic: 'notebook',
    composerControl: 'source',
  })
})

test('retires the removed note mention control from saved help state', () => {
  assert.deepEqual(normalizeProductGuideState({ topic: 'notebook', composerControl: 'mention' }), {
    topic: 'notebook',
    composerControl: 'attachment',
  })
})

test('repairs invalid help state without onboarding semantics', () => {
  assert.deepEqual(normalizeProductGuideState({ topic: 'start', composerControl: 'upload' }), {
    topic: 'composer',
    composerControl: 'attachment',
  })
})

test('usage guide Assistant prompt follows the active UI language', () => {
  assert.match(guideAssistantStarterPrompt('zh-CN'), /准确的界面入口/)
  assert.match(guideAssistantStarterPrompt('en-US'), /exact controls/)
})
