import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./notebookSave.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)

const {
  buildNotebookFolderTree,
  desktopDefaultSavePath,
  notebookPath,
  notebookPathExists,
  relativeNotebookPath,
  resolveGeneratedNotebookEntryType,
} = module.exports

test('saves generated conversation content as chat excerpts', () => {
  assert.equal(resolveGeneratedNotebookEntryType(), 'chat_excerpt')
  assert.equal(resolveGeneratedNotebookEntryType('chat_excerpt'), 'chat_excerpt')
})

test('builds a stable nested Notebook folder tree', () => {
  assert.deepEqual(buildNotebookFolderTree(['research/2026', 'notes', 'research']), [
    { name: 'notes', path: 'notes', children: [] },
    {
      name: 'research',
      path: 'research',
      children: [{ name: '2026', path: 'research/2026', children: [] }],
    },
  ])
})

test('normalizes final Notebook paths and detects case-insensitive conflicts', () => {
  assert.equal(notebookPath('research\\2026', 'Transformer?.md'), 'research/2026/Transformer.md')
  assert.equal(notebookPathExists('Research/Transformer.md', ['research/transformer.md']), true)
})

test('converts a native Vault selection to a Notebook-relative path', () => {
  assert.equal(
    relativeNotebookPath('D:\\Notes', 'd:\\Notes\\research\\Transformer.md'),
    'research/Transformer.md',
  )
  assert.throws(
    () => relativeNotebookPath('D:\\Notes', 'D:\\Other\\Transformer.md'),
    /Vault/,
  )
  assert.equal(relativeNotebookPath('/', '/research/Transformer.md'), 'research/Transformer.md')
})

test('builds the native dialog default path with the Vault separator', () => {
  assert.equal(
    desktopDefaultSavePath('D:\\Notes', 'research/2026', 'Transformer.md'),
    'D:\\Notes\\research\\2026\\Transformer.md',
  )
  assert.equal(desktopDefaultSavePath('/', 'research', 'Transformer.md'), '/research/Transformer.md')
})
