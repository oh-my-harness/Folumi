import { useState, type MouseEvent, type ReactNode } from 'react'
import {
  Check,
  Brain,
  Edit3,
  FileText,
  MessageSquare,
  NotebookPen,
  Pin,
  Settings,
  ShieldOff,
  Sparkles,
  Trash2,
  X,
} from 'lucide-react'
import { useI18n } from '../i18n'
import { openDesktopContextMenu } from '../desktop'

export type AppView = 'assistant' | 'knowledge' | 'notebook' | 'memory' | 'settings'

interface RecentSession {
  id: string
  title: string
  activeRun?: {
    capability?: string
    status?: string
  } | null
  pinned?: boolean
  temporary?: boolean
}

interface Props {
  activeView: AppView
  activeSessionId: string | null
  collapsed: boolean
  recentSessions: RecentSession[]
  onNavigate: (view: AppView) => void
  onSelectSession: (id: string) => void
  onRenameSession: (id: string, title: string) => void
  onDeleteSession: (id: string) => void
  onTogglePinSession: (id: string) => void
}

const navItems: Array<{
  key: AppView
  labelKey: 'nav.assistant' | 'nav.knowledge' | 'nav.notebook' | 'nav.memory'
  icon: typeof MessageSquare
}> = [
  { key: 'assistant', labelKey: 'nav.assistant', icon: MessageSquare },
  { key: 'knowledge', labelKey: 'nav.knowledge', icon: FileText },
  { key: 'notebook', labelKey: 'nav.notebook', icon: NotebookPen },
  { key: 'memory', labelKey: 'nav.memory', icon: Brain },
]

export function Sidebar({
  activeView,
  activeSessionId,
  collapsed,
  recentSessions,
  onNavigate,
  onSelectSession,
  onRenameSession,
  onDeleteSession,
  onTogglePinSession,
}: Props) {
  const { t } = useI18n()
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null)
  const [editingTitle, setEditingTitle] = useState('')

  const startEditing = (session: RecentSession) => {
    setEditingSessionId(session.id)
    setEditingTitle(session.title)
  }

  const submitEditing = () => {
    if (!editingSessionId) return
    const nextTitle = editingTitle.trim()
    if (nextTitle) {
      onRenameSession(editingSessionId, nextTitle)
    }
    setEditingSessionId(null)
    setEditingTitle('')
  }

  const cancelEditing = () => {
    setEditingSessionId(null)
    setEditingTitle('')
  }

  const openSessionContextMenu = (event: MouseEvent, session: RecentSession) => {
    if (editingSessionId === session.id) return
    const opened = openDesktopContextMenu(event.clientX, event.clientY, [
      {
        label: session.pinned ? 'Unpin Session' : 'Pin Session',
        run: () => onTogglePinSession(session.id),
      },
      {
        label: 'Rename Session',
        run: () => startEditing(session),
      },
      {
        label: 'Delete Session',
        run: () => onDeleteSession(session.id),
      },
    ])
    if (opened) {
      event.preventDefault()
      event.stopPropagation()
    }
  }

  return (
    <aside
      className={`app-sidebar flex h-full shrink-0 flex-col border-r transition-[width] duration-200 ${
        collapsed ? 'w-16' : 'w-72'
      }`}
    >
      <div
        data-sidebar-brand="true"
        className={`flex h-14 shrink-0 items-center ${collapsed ? 'justify-center px-2' : 'px-4'}`}
      >
        <button
          type="button"
          className={`flex items-center text-left ${collapsed ? 'justify-center' : 'gap-2.5'}`}
          onClick={() => onNavigate('assistant')}
          title="Folumi"
        >
          <span className="flex h-8 w-8 items-center justify-center rounded-md border border-gray-200 bg-gray-50 text-blue-600">
            <Sparkles size={18} />
          </span>
          {!collapsed && (
            <span className="text-base font-semibold leading-none text-gray-900">Folumi</span>
          )}
        </button>
      </div>

      <nav className={`space-y-1 ${collapsed ? 'px-2' : 'px-3'}`}>
        {navItems.map((item) => {
          const Icon = item.icon
          const active = activeView === item.key
          const label = t(item.labelKey)
          return (
            <button
              key={item.key}
              title={label}
              className={`flex w-full items-center rounded-lg py-2.5 text-left text-sm ${
                collapsed ? 'justify-center px-2' : 'gap-3 px-3'
              } ${
                active
                  ? 'bg-gray-900 text-white'
                  : 'text-gray-700 hover:bg-gray-100 hover:text-gray-900'
              }`}
              onClick={() => onNavigate(item.key)}
            >
              <Icon size={19} />
              {!collapsed && <span>{label}</span>}
            </button>
          )
        })}
      </nav>

      <div className={`mt-6 flex-1 overflow-y-auto ${collapsed ? 'px-2' : 'px-3'}`}>
        {collapsed ? (
          <div className="border-t border-gray-100 pt-3" />
        ) : (
          <>
        <div className="mb-2 px-1 text-xs font-medium uppercase tracking-wide text-gray-500">
          {t('nav.recent')}
        </div>
        <div className="space-y-1">
          {recentSessions.length === 0 ? (
            <div className="rounded-lg px-3 py-2 text-sm text-gray-400">{t('nav.noRecent')}</div>
          ) : (
            recentSessions.map((session) => {
              const editing = editingSessionId === session.id
              const running = Boolean(session.activeRun)
              const selected = activeSessionId === session.id
              return (
                <div
                  key={session.id}
                  data-surface-context-menu="true"
                  onContextMenu={(event) => openSessionContextMenu(event, session)}
                  className={`group relative flex w-full items-center gap-2 rounded-lg border-l-2 px-3 py-2 text-sm ${
                    selected
                      ? 'border-blue-600 bg-gray-100 text-gray-900'
                      : running
                        ? 'border-transparent bg-blue-50/70 text-gray-700 hover:bg-blue-50'
                        : 'border-transparent text-gray-700 hover:bg-gray-100'
                  }`}
                >
                  {!editing && (
                    <button
                      type="button"
                      className="absolute inset-0 z-0 rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-blue-300 focus-visible:ring-inset"
                      onClick={() => onSelectSession(session.id)}
                      aria-current={selected ? 'page' : undefined}
                      aria-label={`Open session: ${session.title}`}
                      title={[session.title, selected ? 'Current session' : '', session.pinned ? 'Pinned' : '', running ? 'Running' : ''].filter(Boolean).join(' · ')}
                    />
                  )}
                  <div className="pointer-events-none relative z-10 shrink-0">
                    {session.temporary
                      ? <ShieldOff size={16} className={running ? 'text-blue-600' : 'text-amber-600'} />
                      : <FileText size={16} className={running ? 'text-blue-600' : 'text-gray-500'} />}
                    {running && (
                      <span
                        className="absolute -right-1 -top-1 h-2.5 w-2.5 rounded-full bg-blue-500 ring-2 ring-white"
                        title="Agent running"
                      >
                        <span className="absolute inset-0 animate-ping rounded-full bg-blue-400" />
                      </span>
                    )}
                  </div>
                  {editing ? (
                    <>
                      <input
                        className="relative z-10 min-w-0 flex-1 rounded border border-gray-300 bg-white px-2 py-1 text-sm outline-none focus:border-gray-900"
                        value={editingTitle}
                        autoFocus
                        onChange={(event) => setEditingTitle(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') submitEditing()
                          if (event.key === 'Escape') cancelEditing()
                        }}
                      />
                      <IconButton label="Save session name" onClick={submitEditing}>
                        <Check size={15} />
                      </IconButton>
                      <IconButton label="Cancel rename" onClick={cancelEditing}>
                        <X size={15} />
                      </IconButton>
                    </>
                  ) : (
                    <>
                      <div className="pointer-events-none relative z-10 min-w-0 flex-1 text-left">
                        <span className="flex min-w-0 items-center gap-1">
                          {session.pinned && <Pin size={12} className="shrink-0 text-gray-500" />}
                          <span className="block min-w-0 truncate">{session.title}</span>
                        </span>
                        {running && (
                          <span className="mt-0.5 block truncate text-[11px] font-medium text-blue-600">
                            Running{session.activeRun?.capability ? ` · ${capabilityLabel(session.activeRun.capability)}` : ''}
                          </span>
                        )}
                        {!running && session.temporary && (
                          <span className="mt-0.5 block truncate text-[11px] font-medium text-amber-700">
                            {t('chat.temporary.sidebar')}
                          </span>
                        )}
                      </div>
                      <div className="pointer-events-none relative z-20 flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                        <IconButton label="Rename session" onClick={() => startEditing(session)}>
                          <Edit3 size={15} />
                        </IconButton>
                        <IconButton label="Delete session" onClick={() => onDeleteSession(session.id)}>
                          <Trash2 size={15} />
                        </IconButton>
                      </div>
                    </>
                  )}
                </div>
              )
            })
          )}
        </div>
          </>
        )}
      </div>

      <div className={`border-t border-gray-200 ${collapsed ? 'p-2' : 'p-3'}`}>
        <button
          title={t('nav.settings')}
          className={`flex w-full items-center rounded-lg py-2.5 text-left text-sm ${
            collapsed ? 'justify-center px-2' : 'gap-3 px-3'
          } ${
            activeView === 'settings'
              ? 'bg-gray-900 text-white'
              : 'text-gray-700 hover:bg-gray-100 hover:text-gray-900'
          }`}
          onClick={() => onNavigate('settings')}
        >
          <Settings size={19} />
          {!collapsed && <span>{t('nav.settings')}</span>}
        </button>
        {!collapsed && <div className="mt-3 px-3 text-xs text-gray-500">v{__APP_VERSION__}</div>}
      </div>
    </aside>
  )
}

function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={label}
      className="pointer-events-auto rounded p-1 text-gray-500 hover:bg-gray-200 hover:text-gray-900"
      onClick={(event) => {
        event.stopPropagation()
        onClick()
      }}
    >
      {children}
    </button>
  )
}

function capabilityLabel(value: string) {
  if (value === 'deep_solve') return 'Deep Solve'
  if (value === 'code_exec') return 'Code Exec'
  if (value === 'quiz') return 'Quiz'
  if (value === 'organize') return 'Organize'
  return 'Chat'
}
