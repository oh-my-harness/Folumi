import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = readFileSync(new URL('./settings.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
Function('module', 'exports', compiled)(module, module.exports)
const {
  defaultLlmSettings,
  completeOnboardingSettings,
  hasUsableLlmConfig,
  normalizeTheme,
  settingsForSession,
  settingsRequireSessionReset,
  shouldShowOnboarding,
  withModelThinkingLevel,
} = module.exports

test('keeps supported appearance themes', () => {
  assert.equal(normalizeTheme('cool-light'), 'cool-light')
  assert.equal(normalizeTheme('graphite-dark'), 'graphite-dark')
})

test('migrates missing and unknown theme values to cool light', () => {
  assert.equal(normalizeTheme(undefined), 'cool-light')
  assert.equal(normalizeTheme('legacy-dark'), 'cool-light')
})

test('theme changes do not reset the active runtime session', () => {
  const darkSettings = { ...defaultLlmSettings, theme: 'graphite-dark' }
  assert.equal(settingsRequireSessionReset(defaultLlmSettings, darkSettings), false)
  assert.equal(settingsRequireSessionReset(darkSettings, darkSettings), false)
  assert.equal(
    settingsRequireSessionReset(darkSettings, { ...darkSettings, model: 'another-model' }),
    true,
  )
})

test('onboarding progress does not reset the active runtime session', () => {
  const completed = {
    ...defaultLlmSettings,
    onboardingCompleted: true,
    dismissedContextHints: ['notebook.empty'],
  }
  assert.equal(settingsRequireSessionReset(defaultLlmSettings, completed), false)
})

test('shows onboarding once per required version and completes it without changing product data', () => {
  assert.equal(shouldShowOnboarding(defaultLlmSettings), true)
  const completed = completeOnboardingSettings(defaultLlmSettings)
  assert.equal(completed.onboardingCompleted, true)
  assert.equal(shouldShowOnboarding(completed), false)
  assert.equal(shouldShowOnboarding(completed, completed.onboardingVersion + 1), true)
  assert.deepEqual(completed.llmConfigs, defaultLlmSettings.llmConfigs)
})

test('requires a complete active model profile before onboarding marks model readiness', () => {
  const profile = {
    id: 'model-a',
    name: 'Model A',
    provider: 'openai',
    model: 'model-a-name',
    apiKey: 'key-a',
    baseUrl: 'https://a.example',
    chatPath: '/v1/chat/completions',
    contextWindowTokens: 64000,
  }
  assert.equal(hasUsableLlmConfig({
    ...defaultLlmSettings,
    llmConfigs: [profile],
    activeLlmConfigId: profile.id,
  }), true)
  assert.equal(hasUsableLlmConfig({
    ...defaultLlmSettings,
    llmConfigs: [{ ...profile, apiKey: '' }],
    activeLlmConfigId: profile.id,
  }), false)
})

test('builds runtime settings from an explicitly selected model config', () => {
  const settings = {
    ...defaultLlmSettings,
    llmConfigs: [
      {
        id: 'model-a',
        name: 'Model A',
        provider: 'openai',
        model: 'model-a-name',
        apiKey: 'key-a',
        baseUrl: 'https://a.example',
        chatPath: '/v1/chat/completions',
        contextWindowTokens: 64000,
        thinkingLevel: 'low',
      },
      {
        id: 'model-b',
        name: 'Model B',
        provider: 'anthropic',
        model: 'model-b-name',
        apiKey: 'key-b',
        baseUrl: 'https://b.example',
        chatPath: '',
        contextWindowTokens: 200000,
        thinkingLevel: 'high',
      },
    ],
    activeLlmConfigId: 'model-a',
  }

  const selected = settingsForSession(settings, 'model-b')
  assert.equal(selected.provider, 'anthropic')
  assert.equal(selected.model, 'model-b-name')
  assert.equal(selected.api_key, 'key-b')
  assert.equal(selected.context_window_tokens, 200000)
  assert.equal(selected.thinking_level, 'high')
  assert.equal(settingsForSession(settings, 'model-b', 'minimal').thinking_level, 'minimal')
  assert.equal('budget_limit_usd' in selected, false)
  assert.equal('budgetLimitUsd' in defaultLlmSettings, false)
})

test('changing a model thinking level resets the runtime session', () => {
  const profile = {
    id: 'model-a',
    name: 'Model A',
    provider: 'openai',
    model: 'model-a-name',
    apiKey: 'key-a',
    baseUrl: 'https://a.example',
    chatPath: '/v1/chat/completions',
    contextWindowTokens: 64000,
    thinkingLevel: 'off',
  }
  const current = { ...defaultLlmSettings, llmConfigs: [profile], activeLlmConfigId: profile.id }
  const next = { ...current, llmConfigs: [{ ...profile, thinkingLevel: 'medium' }] }
  assert.equal(settingsRequireSessionReset(current, next), true)
})

test('remembers the composer thinking level only for the selected model', () => {
  const first = { ...defaultLlmSettings.llmConfigs[0], id: 'first', thinkingLevel: 'off' }
  const second = { ...first, id: 'second', thinkingLevel: 'low' }
  const current = { ...defaultLlmSettings, llmConfigs: [first, second], activeLlmConfigId: first.id }

  const next = withModelThinkingLevel(current, first.id, 'xhigh')
  assert.equal(next.llmConfigs[0].thinkingLevel, 'xhigh')
  assert.equal(next.llmConfigs[1].thinkingLevel, 'low')
  assert.equal(withModelThinkingLevel(current, 'missing', 'high'), current)
})
