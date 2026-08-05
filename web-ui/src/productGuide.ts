import type { UiLanguage } from './i18n'

export type ProductGuideTopic =
  | 'composer'
  | 'materials'
  | 'knowledge'
  | 'notebook'
  | 'memory'

export type ComposerGuideControl = 'attachment' | 'source' | 'mention' | 'model' | 'send'

export interface ProductGuideState {
  topic: ProductGuideTopic
  composerControl: ComposerGuideControl
}

export type ProductGuideDestination =
  | 'chat'
  | 'knowledge'
  | 'notebook'
  | 'memory'
  | 'embedding-settings'
  | 'notebook-settings'

export const productGuideTopics: ProductGuideTopic[] = [
  'composer',
  'materials',
  'knowledge',
  'notebook',
  'memory',
]

export const composerGuideControls: ComposerGuideControl[] = [
  'attachment',
  'source',
  'mention',
  'model',
  'send',
]

export const defaultProductGuideState: ProductGuideState = {
  topic: 'composer',
  composerControl: 'attachment',
}

const PRODUCT_GUIDE_STORAGE_KEY = 'folumi.productGuideState'

export function normalizeProductGuideState(value: unknown): ProductGuideState {
  const candidate = value && typeof value === 'object' ? value as Record<string, unknown> : {}
  return {
    topic: isProductGuideTopic(candidate.topic) ? candidate.topic : defaultProductGuideState.topic,
    composerControl: isComposerGuideControl(candidate.composerControl)
      ? candidate.composerControl
      : defaultProductGuideState.composerControl,
  }
}

export function loadProductGuideState(): ProductGuideState {
  try {
    const raw = localStorage.getItem(PRODUCT_GUIDE_STORAGE_KEY)
    return raw ? normalizeProductGuideState(JSON.parse(raw)) : defaultProductGuideState
  } catch {
    return defaultProductGuideState
  }
}

export function saveProductGuideState(state: ProductGuideState) {
  try {
    localStorage.setItem(PRODUCT_GUIDE_STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Help remains usable when browser storage is unavailable.
  }
}

export function guideAssistantStarterPrompt(language: UiLanguage) {
  return language === 'en-US'
    ? 'I need help using Folumi. First ask what I am trying to accomplish, then point me to the exact controls.'
    : '我需要了解如何使用 Folumi。请先问我想完成什么，再告诉我准确的界面入口。'
}

function isProductGuideTopic(value: unknown): value is ProductGuideTopic {
  return typeof value === 'string' && productGuideTopics.includes(value as ProductGuideTopic)
}

function isComposerGuideControl(value: unknown): value is ComposerGuideControl {
  return typeof value === 'string' && composerGuideControls.includes(value as ComposerGuideControl)
}
