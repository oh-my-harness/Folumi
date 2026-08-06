import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./notebookLivePreview.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)

const { replaceNotebookMarkdownBlock, splitNotebookMarkdownBlocks } = module.exports

test('splits headings and paragraphs without changing Markdown bytes', () => {
  const markdown = '# 标题\n正文第一行\n正文第二行\n\n## 小节\n\n结尾\n'
  const blocks = splitNotebookMarkdownBlocks(markdown)
  assert.equal(blocks.map((block) => block.source + block.separator).join(''), markdown)
  assert.deepEqual(blocks.map((block) => block.source), [
    '# 标题',
    '正文第一行\n正文第二行\n',
    '## 小节',
    '结尾\n',
  ])
})

test('keeps fenced code containing blank lines in one live preview block', () => {
  const markdown = '```ts\nconst value = 1\n\nconsole.log(value)\n```\n\nAfter\n'
  const blocks = splitNotebookMarkdownBlocks(markdown)
  assert.equal(blocks.length, 2)
  assert.match(blocks[0].source, /console\.log/)
  assert.equal(blocks.map((block) => block.source + block.separator).join(''), markdown)
})

test('replaces only the active Markdown block', () => {
  const markdown = '# Old\n\nBody'
  const [heading] = splitNotebookMarkdownBlocks(markdown)
  assert.equal(
    replaceNotebookMarkdownBlock(markdown, heading.start, heading.end, '# New'),
    '# New\n\nBody',
  )
})
