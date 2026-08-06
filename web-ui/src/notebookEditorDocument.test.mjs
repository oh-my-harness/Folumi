import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./notebookEditorDocument.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)

const { joinNotebookEditorDocument, splitNotebookEditorDocument } = module.exports

test('keeps leading YAML Front Matter byte-for-byte outside the editable body', () => {
  const markdown = '---\r\ntitle: Test\r\ntags:\r\n  - one\r\n---\r\n# Body\r\n'
  const document = splitNotebookEditorDocument(markdown)
  assert.equal(document.frontMatter, '---\r\ntitle: Test\r\ntags:\r\n  - one\r\n---\r\n')
  assert.equal(document.body, '# Body\r\n')
  assert.equal(joinNotebookEditorDocument(document.frontMatter, document.body), markdown)
})

test('leaves a thematic break in ordinary Markdown inside the editable body', () => {
  const markdown = '# Body\n\n---\n\nAfter'
  assert.deepEqual(splitNotebookEditorDocument(markdown), { frontMatter: '', body: markdown })
})
