import { getCurrentWindow } from '@tauri-apps/api/window'
import { Minus, PanelLeftClose, PanelLeftOpen, Square, X } from 'lucide-react'
import type { MouseEvent } from 'react'
import { useI18n } from '../i18n'

interface Props {
  sidebarCollapsed: boolean
  onToggleSidebar: () => void
}

export function AppTitleBar({ sidebarCollapsed, onToggleSidebar }: Props) {
  const { language, t } = useI18n()

  const stopTitleBarDoubleClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation()
  }

  return (
    <header
      className="app-titlebar relative flex h-10 shrink-0 items-center border-b"
      data-app-titlebar="true"
      data-tauri-drag-region
      onDoubleClick={() => runWindowAction('toggle maximize', (window) => window.toggleMaximize())}
    >
      <button
        type="button"
        className="app-titlebar-sidebar-toggle absolute left-3 top-1/2 flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-md text-[var(--text-muted)] transition-colors hover:bg-[var(--theme-surface-hover)] hover:text-[var(--text-primary)]"
        title={sidebarCollapsed ? t('nav.expand') : t('nav.collapse')}
        aria-label={sidebarCollapsed ? t('nav.expand') : t('nav.collapse')}
        onClick={onToggleSidebar}
        onDoubleClick={stopTitleBarDoubleClick}
      >
        {sidebarCollapsed ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
      </button>

      <div className="absolute inset-y-0 right-0 flex" data-window-controls="true">
          <WindowControlButton
            label={language === 'en-US' ? 'Minimize' : '最小化'}
            onClick={() => runWindowAction('minimize', (window) => window.minimize())}
          >
            <Minus size={16} />
          </WindowControlButton>
          <WindowControlButton
            label={language === 'en-US' ? 'Maximize or restore' : '最大化或还原'}
            onClick={() => runWindowAction('toggle maximize', (window) => window.toggleMaximize())}
          >
            <Square size={13} />
          </WindowControlButton>
          <WindowControlButton
            label={language === 'en-US' ? 'Close' : '关闭'}
            close
            onClick={() => runWindowAction('close', (window) => window.close())}
          >
            <X size={17} />
          </WindowControlButton>
      </div>
    </header>
  )
}

function WindowControlButton({
  label,
  close = false,
  onClick,
  children,
}: {
  label: string
  close?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      className={`flex h-full w-12 items-center justify-center transition-colors ${
        close
          ? 'text-[var(--text-muted)] hover:bg-red-600 hover:text-white'
          : 'text-[var(--text-muted)] hover:bg-[var(--theme-surface-hover)] hover:text-[var(--text-primary)]'
      }`}
      title={label}
      aria-label={label}
      onClick={onClick}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      {children}
    </button>
  )
}

type CurrentWindow = ReturnType<typeof getCurrentWindow>

function runWindowAction(label: string, action: (window: CurrentWindow) => Promise<void>) {
  try {
    void action(getCurrentWindow()).catch((error) => {
      console.error(`Failed to ${label}`, error)
    })
  } catch {
    // Browser preview has no native window; title-bar controls are inert there.
  }
}
