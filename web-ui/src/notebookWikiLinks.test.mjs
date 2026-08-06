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
  notebookWikiTargetFromHref,
  prepareNotebookMarkdownForEditor,
  restoreNotebookMarkdownFromEditor,
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

test('round trips Wiki links and embedded assets through editor-safe links', () => {
  const markdown = 'See [[Breakfast]] and [[person/name|Name]].\n\n![[image.png]]\n'
  const prepared = prepareNotebookMarkdownForEditor(markdown)
  assert.match(prepared, /#folumi-wiki-/)
  assert.match(prepared, /#folumi-embed-/)
  assert.equal(restoreNotebookMarkdownFromEditor(prepared), markdown)
})

test('does not transform Wiki-like examples inside code', () => {
  const markdown = '`[[inline]]`\n\n```md\n[[fenced]]\n```\n\n[[real]]'
  const prepared = prepareNotebookMarkdownForEditor(markdown)
  assert.match(prepared, /`\[\[inline\]\]`/)
  assert.match(prepared, /\n\[\[fenced\]\]\n/)
  assert.match(prepared, /#folumi-wiki-real/)
  assert.equal(restoreNotebookMarkdownFromEditor(prepared), markdown)
})

test('gets the navigation target without exposing the alias', () => {
  assert.equal(notebookWikiTargetFromHref('#folumi-wiki-person%2Fname%7CName'), 'person/name')
  assert.equal(notebookWikiTargetFromHref('https://example.com'), undefined)
})
