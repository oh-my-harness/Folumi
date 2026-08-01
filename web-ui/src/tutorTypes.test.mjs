import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./tutorTypes.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)
const {
  assertTutorMutationContract,
  fetchTutorMemory,
  normalizeTutorProfile,
  resolveTutorMemory,
} = module.exports

test('keeps native Soul Markdown unchanged', () => {
  const profile = normalizeTutorProfile({ soul_markdown: '# Identity\n\nTeach visually.' })
  assert.equal(profile.soul_markdown, '# Identity\n\nTeach visually.')
})

test('maps only stable legacy role fields into Soul Markdown', () => {
  const profile = normalizeTutorProfile({ role: 'Teach math', goal: 'Learn algebra' })
  assert.match(profile.soul_markdown, /Teach math/)
  assert.doesNotMatch(profile.soul_markdown, /Learn algebra/)
})

test('rejects writes acknowledged by a legacy Tutor backend', () => {
  assert.throws(
    () => assertTutorMutationContract({ role: 'Old role', goal: 'Old goal' }),
    /重启 Folumi/,
  )
  assert.doesNotThrow(() => assertTutorMutationContract({ soul_markdown: '# Identity' }))
})

test('uses tutor-scoped memory endpoints', async () => {
  const originalFetch = globalThis.fetch
  const requests = []
  globalThis.fetch = async (url, init = {}) => {
    requests.push({ url, method: init.method ?? 'GET' })
    if (String(url).includes('/resolve')) {
      return new Response(JSON.stringify({ id: 'entry-1', status: 'resolved' }), { status: 200 })
    }
    return new Response(JSON.stringify({ entries: [{ id: 'entry-1', tutor_id: 'tutor-a' }] }), { status: 200 })
  }
  try {
    const entries = await fetchTutorMemory('tutor-a', true)
    assert.equal(entries[0].tutor_id, 'tutor-a')
    await resolveTutorMemory('tutor-a', 'entry-1')
    assert.deepEqual(requests, [
      { url: '/api/tutors/tutor-a/memory?include_resolved=true', method: 'GET' },
      { url: '/api/tutors/tutor-a/memory/entry-1/resolve', method: 'POST' },
    ])
  } finally {
    globalThis.fetch = originalFetch
  }
})
