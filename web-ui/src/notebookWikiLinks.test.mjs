import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./notebookWikiLinks.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)

const {
  findNotebookWikiLinks,
  notebookWikiTargetAtOffset,
} = module.exports

test('finds Wiki links and keeps aliases out of their navigation target', () => {
  assert.deepEqual(findNotebookWikiLinks('See [[早餐]] and [[person/name|姓名]].'), [
    { from: 4, to: 10, target: '早餐' },
    { from: 15, to: 33, target: 'person/name' },
  ])
})

test('does not turn embedded assets or empty targets into note links', () => {
  assert.deepEqual(findNotebookWikiLinks('![[image.png]] [[]] [[  ]]'), [])
})

test('gets the raw Wiki target at the clicked source offset', () => {
  const text = 'See [[person/name|Name]] and ![[asset.png]].'
  assert.equal(notebookWikiTargetAtOffset(text, text.indexOf('Name')), 'person/name')
  assert.equal(notebookWikiTargetAtOffset(text, text.indexOf('asset')), undefined)
})
