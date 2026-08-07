import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./knowledgeCitation.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)
const { knowledgeCitationsFromTrace } = module.exports

test('maps a runtime knowledge read citation to a navigable product citation', () => {
  const citations = knowledgeCitationsFromTrace({
    kind: 'tool_result',
    tool: 'knowledge_read',
    ok: true,
    details: {
      citation: {
        handle: '[K:run-scope:1]',
        reference: {
          source_id: 'course-knowledge',
          item_id: 'chunk_opaque',
          revision: 'sha256:revision',
        },
      },
      uri: 'kb:kb-a:document-a:chunk-0',
      truncated: false,
    },
  })

  assert.deepEqual(citations, [{
    index: 0,
    source: 'document-a',
    text: 'Verified course evidence [K:run-scope:1]',
    kind: 'rag',
    title: 'document-a · chunk-0',
    kb: 'kb-a',
    documentId: 'document-a',
    chunkId: 'chunk-0',
    rawSource: 'kb:kb-a:document-a:chunk-0',
  }])
})

test('ignores failed or unverified knowledge read payloads', () => {
  assert.deepEqual(knowledgeCitationsFromTrace({
    kind: 'tool_result',
    tool: 'knowledge_read',
    ok: false,
    details: {},
  }), [])
  assert.deepEqual(knowledgeCitationsFromTrace({
    kind: 'tool_result',
    tool: 'knowledge_read',
    ok: true,
    details: {
      citation: {
        handle: '[K:forged:1]',
        reference: {},
      },
    },
  }), [])
})

test('maps a runtime history read to its source conversation', () => {
  const citations = knowledgeCitationsFromTrace({
    kind: 'tool_result',
    tool: 'knowledge_read',
    ok: true,
    details: {
      citation: {
        handle: '[K:run-scope:2]',
        reference: {
          source_id: 'session_recall',
          item_id: 'runtime-opaque-reference',
          revision: '7',
        },
      },
      uri: 'chat:session-a:user-entry-a',
      truncated: false,
    },
  })

  assert.deepEqual(citations, [{
    index: 0,
    source: 'session-a',
    text: '过往对话来源 [K:run-scope:2]',
    kind: 'rag',
    title: '历史对话 · session-a',
    kb: undefined,
    documentId: undefined,
    chunkId: undefined,
    rawSource: 'chat:session-a:user-entry-a',
  }])
})
