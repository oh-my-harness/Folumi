import { openExternalUrl, readClipboardText, writeClipboardText } from './api'

let initialized = false
let contextMenuElement: HTMLDivElement | null = null

export interface DesktopContextAction {
  label: string
  run?: () => void
  disabled?: boolean
}

export function openDesktopContextMenu(x: number, y: number, actions: DesktopContextAction[]) {
  if (document.documentElement.dataset.desktop !== 'true') return false
  showDesktopContextMenu(x, y, actions)
  return true
}

export async function initializeDesktopBehavior() {
  if (initialized) return
  initialized = true

  const { isTauri } = await import('@tauri-apps/api/core')
  if (!isTauri()) return

  document.documentElement.dataset.desktop = 'true'
  installDesktopContextMenu()
  installExternalLinkHandling()
  installDesktopDragDropHandling()
}

function installDesktopContextMenu() {
  document.addEventListener('contextmenu', (event) => {
    const target = event.target
    if (!(target instanceof Element)) return
    if (target.closest('[data-browser-context-menu="true"]')) return
    if (target.closest('[data-surface-context-menu="true"]')) return

    event.preventDefault()
    showDesktopContextMenu(event.clientX, event.clientY, desktopContextActions(target))
  }, true)

  document.addEventListener('click', hideDesktopContextMenu, true)
  window.addEventListener('blur', hideDesktopContextMenu)
  window.addEventListener('resize', hideDesktopContextMenu)
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') hideDesktopContextMenu()
  })
}

function installExternalLinkHandling() {
  document.addEventListener('click', (event) => {
    const target = event.target
    if (!(target instanceof Element)) return
    const anchor = target.closest('a[href]')
    if (!(anchor instanceof HTMLAnchorElement)) return
    const href = anchor.getAttribute('href') ?? ''
    if (!isExternalHref(href)) return

    event.preventDefault()
    void openExternalUrl(anchor.href).catch((error) => {
      console.error('Failed to open external URL', error)
      window.open(anchor.href, '_blank', 'noopener,noreferrer')
    })
  }, true)
}

function installDesktopDragDropHandling() {
  const preventBrowserFileDrop = (event: DragEvent) => {
    if (event.dataTransfer?.types.includes('Files')) {
      event.preventDefault()
    }
  }

  document.addEventListener('dragover', preventBrowserFileDrop, true)
  document.addEventListener('drop', preventBrowserFileDrop, true)
}

function desktopContextActions(target: Element): DesktopContextAction[] {
  const editableActions = editableContextActions(target)
  if (editableActions.length > 0) return editableActions

  const selection = window.getSelection()?.toString().trim() ?? ''
  const actions: DesktopContextAction[] = []

  if (selection) {
    actions.push({
      label: 'Copy',
      run: () => {
        void writeClipboardText(selection)
      },
    })
  }

  const link = target.closest('a[href]')
  if (link instanceof HTMLAnchorElement && isExternalHref(link.getAttribute('href') ?? '')) {
    actions.push({
      label: 'Open Link',
      run: () => {
        void openExternalUrl(link.href)
      },
    })
    actions.push({
      label: 'Copy Link',
      run: () => {
        void writeClipboardText(link.href)
      },
    })
  }

  return actions.length > 0 ? actions : [{ label: 'No actions available', disabled: true }]
}

function editableContextActions(target: Element): DesktopContextAction[] {
  const editable = editableElement(target)
  if (!editable) return []

  const selectedText = editableSelectedText(editable)
  return [
    {
      label: 'Cut',
      disabled: !selectedText || isReadOnlyEditable(editable),
      run: () => {
        void writeClipboardText(selectedText)
        replaceEditableSelection(editable, '')
      },
    },
    {
      label: 'Copy',
      disabled: !selectedText,
      run: () => {
        void writeClipboardText(selectedText)
      },
    },
    {
      label: 'Paste',
      disabled: isReadOnlyEditable(editable),
      run: () => {
        void readClipboardText().then((text) => {
          if (text) replaceEditableSelection(editable, text)
        })
      },
    },
    {
      label: 'Select All',
      run: () => selectEditableText(editable),
    },
  ]
}

function showDesktopContextMenu(x: number, y: number, actions: DesktopContextAction[]) {
  hideDesktopContextMenu()
  const menu = document.createElement('div')
  menu.className = 'desktop-context-menu'

  for (const action of actions) {
    const button = document.createElement('button')
    button.type = 'button'
    button.textContent = action.label
    button.disabled = Boolean(action.disabled)
    button.addEventListener('click', () => {
      hideDesktopContextMenu()
      action.run?.()
    })
    menu.appendChild(button)
  }

  document.body.appendChild(menu)
  const rect = menu.getBoundingClientRect()
  const left = Math.min(x, window.innerWidth - rect.width - 8)
  const top = Math.min(y, window.innerHeight - rect.height - 8)
  menu.style.left = `${Math.max(8, left)}px`
  menu.style.top = `${Math.max(8, top)}px`
  contextMenuElement = menu
}

function hideDesktopContextMenu() {
  contextMenuElement?.remove()
  contextMenuElement = null
}

function isExternalHref(href: string) {
  const value = href.trim()
  if (!value || value.startsWith('#') || value.startsWith('/') || value.startsWith('./') || value.startsWith('../')) {
    return false
  }
  try {
    const url = new URL(value)
    return url.protocol === 'http:' || url.protocol === 'https:' || url.protocol === 'mailto:'
  } catch {
    return false
  }
}

type EditableElement = HTMLInputElement | HTMLTextAreaElement | HTMLElement

function editableElement(target: Element): EditableElement | null {
  const editable = target.closest('input, textarea, [contenteditable="true"]')
  if (editable instanceof HTMLInputElement) return isTextInput(editable) ? editable : null
  if (editable instanceof HTMLTextAreaElement) return editable
  if (editable instanceof HTMLElement && editable.isContentEditable) return editable
  return null
}

function isTextInput(input: HTMLInputElement) {
  return [
    '',
    'email',
    'number',
    'password',
    'search',
    'tel',
    'text',
    'url',
  ].includes(input.type)
}

function editableSelectedText(editable: EditableElement) {
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) {
    const start = editable.selectionStart ?? 0
    const end = editable.selectionEnd ?? 0
    return editable.value.slice(start, end)
  }
  return window.getSelection()?.toString() ?? ''
}

function replaceEditableSelection(editable: EditableElement, text: string) {
  editable.focus()
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) {
    const start = editable.selectionStart ?? editable.value.length
    const end = editable.selectionEnd ?? editable.value.length
    const nextValue = `${editable.value.slice(0, start)}${text}${editable.value.slice(end)}`
    editable.value = nextValue
    editable.setSelectionRange(start + text.length, start + text.length)
    editable.dispatchEvent(new Event('input', { bubbles: true }))
    return
  }
  document.execCommand('insertText', false, text)
}

function selectEditableText(editable: EditableElement) {
  editable.focus()
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) {
    editable.select()
    return
  }
  const range = document.createRange()
  range.selectNodeContents(editable)
  const selection = window.getSelection()
  selection?.removeAllRanges()
  selection?.addRange(range)
}

function isReadOnlyEditable(editable: EditableElement) {
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) {
    return editable.readOnly || editable.disabled
  }
  return editable.getAttribute('contenteditable') === 'false'
}
