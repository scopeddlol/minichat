import { X } from 'lucide-react'
import {
  createContext, useCallback, useContext, useEffect, useId, useRef, useState,
  type ReactNode,
} from 'react'
import { createPortal } from 'react-dom'
import { create } from 'zustand'
import { avatarGradient, initials } from '../lib/format'

/* -------------------------------------------------------------------------- */
/* Avatar                                                                      */
/* -------------------------------------------------------------------------- */

const SIZES = { xs: 20, sm: 28, md: 36, lg: 44, xl: 80, xxl: 112 } as const

export function Avatar({
  name,
  id,
  src,
  accent = '#5b6ee8',
  size = 'md',
  presence,
  ring,
  className = '',
}: {
  name: string
  id: string
  src?: string | null
  accent?: string
  size?: keyof typeof SIZES
  presence?: 'online' | 'idle' | 'dnd' | 'offline'
  ring?: boolean
  className?: string
}) {
  const [broken, setBroken] = useState(false)
  const pixels = SIZES[size]
  const dot = Math.max(8, Math.round(pixels * 0.28))

  return (
    <div className={`relative shrink-0 ${className}`} style={{ width: pixels, height: pixels }}>
      <div
        className="w-full h-full rounded-full overflow-hidden flex items-center justify-center font-semibold text-white select-none"
        style={{
          background: src && !broken ? undefined : avatarGradient(id, accent),
          fontSize: pixels * 0.38,
          boxShadow: ring ? `0 0 0 2px var(--accent)` : undefined,
        }}
      >
        {src && !broken ? (
          <img
            src={src}
            alt=""
            loading="lazy"
            className="w-full h-full object-cover"
            onError={() => setBroken(true)}
          />
        ) : (
          initials(name)
        )}
      </div>
      {presence && (
        <span
          className="absolute rounded-full"
          style={{
            width: dot,
            height: dot,
            right: -1,
            bottom: -1,
            background: presenceColor(presence),
            boxShadow: '0 0 0 2.5px var(--surface-1)',
          }}
          title={presenceLabel(presence)}
        />
      )}
    </div>
  )
}

export function presenceColor(presence: string): string {
  switch (presence) {
    case 'online':
      return 'var(--success)'
    case 'idle':
      return 'var(--warning)'
    case 'dnd':
      return 'var(--danger)'
    default:
      return 'var(--text-faint)'
  }
}

export function presenceLabel(presence: string): string {
  switch (presence) {
    case 'online':
      return 'Online'
    case 'idle':
      return 'Idle'
    case 'dnd':
      return 'Do not disturb'
    default:
      return 'Offline'
  }
}

/* -------------------------------------------------------------------------- */
/* Modal                                                                       */
/* -------------------------------------------------------------------------- */

/**
 * Open modals, outermost first.
 *
 * Shared by every `<Modal />` so Escape only reaches the topmost one.
 */
const modalStack: object[] = []

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 'md',
  bare = false,
}: {
  open: boolean
  onClose: () => void
  title?: ReactNode
  description?: ReactNode
  children: ReactNode
  footer?: ReactNode
  width?: 'sm' | 'md' | 'lg' | 'xl' | 'full'
  bare?: boolean
}) {
  const titleId = useId()

  useEffect(() => {
    if (!open) return

    // Modals nest — the admin panel opens the channel dialog, and both are
    // Modals. Only the last one opened may act on Escape, or one keypress
    // closes the dialog *and* the panel behind it.
    const token = {}
    modalStack.push(token)

    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      if (modalStack[modalStack.length - 1] !== token) return
      // A dropdown open inside the modal owns Escape first — dismissing a
      // picker shouldn't also throw away the dialog you were filling in.
      if (document.querySelector('[role="listbox"]')) return
      // So does a right-click menu, which handles Escape in capture phase.
      if (document.querySelector('[role="menu"]')) return
      onClose()
    }
    document.addEventListener('keydown', onKey)
    const previous = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKey)
      const at = modalStack.indexOf(token)
      if (at !== -1) modalStack.splice(at, 1)
      // Only the outermost modal should give scrolling back; an inner one
      // closing must leave the page locked for the one still open.
      if (modalStack.length === 0) document.body.style.overflow = previous
    }
  }, [open, onClose])

  if (!open) return null

  const maxWidth = {
    sm: '380px',
    md: '520px',
    lg: '720px',
    xl: '980px',
    full: 'min(1180px, 96vw)',
  }[width]

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 animate-fade-in"
      style={{ background: 'rgb(0 0 0 / 0.62)', backdropFilter: 'blur(3px)' }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? titleId : undefined}
        className="w-full card animate-pop-in flex flex-col max-h-[90vh] overflow-hidden"
        style={{ maxWidth, boxShadow: 'var(--shadow-lg)' }}
      >
        {!bare && (title || description) && (
          <header className="px-5 pt-5 pb-4 border-b flex items-start gap-3">
            <div className="min-w-0 flex-1">
              {title && (
                <h2 id={titleId} className="text-[1.05rem] font-semibold leading-tight">
                  {title}
                </h2>
              )}
              {description && (
                <p className="text-sm mt-1" style={{ color: 'var(--text-muted)' }}>
                  {description}
                </p>
              )}
            </div>
            <button className="btn btn-ghost !p-1.5" onClick={onClose} aria-label="Close">
              <X size={18} />
            </button>
          </header>
        )}
        <div className="overflow-y-auto scroll-thin flex-1">{children}</div>
        {footer && <footer className="px-5 py-4 border-t flex justify-end gap-2">{footer}</footer>}
      </div>
    </div>,
    document.body,
  )
}

/* -------------------------------------------------------------------------- */
/* Toasts                                                                      */
/* -------------------------------------------------------------------------- */

type ToastKind = 'info' | 'success' | 'error'
interface Toast {
  id: number
  message: string
  kind: ToastKind
}

interface ToastStore {
  toasts: Toast[]
  push: (message: string, kind?: ToastKind) => void
  dismiss: (id: number) => void
}

let toastId = 0
export const useToasts = create<ToastStore>((set) => ({
  toasts: [],
  push(message, kind = 'info') {
    const id = ++toastId
    set((s) => ({ toasts: [...s.toasts, { id, message, kind }] }))
    setTimeout(() => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })), 4200)
  },
  dismiss(id) {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }))
  },
}))

export const toast = {
  info: (message: string) => useToasts.getState().push(message, 'info'),
  success: (message: string) => useToasts.getState().push(message, 'success'),
  error: (message: string) => useToasts.getState().push(message, 'error'),
}

export function ToastViewport() {
  const { toasts, dismiss } = useToasts()
  if (!toasts.length) return null

  return createPortal(
    <div className="fixed z-[60] bottom-24 sm:bottom-6 left-1/2 sm:left-auto sm:right-6 -translate-x-1/2 sm:translate-x-0 flex flex-col gap-2 items-center px-4 sm:px-0 w-full max-w-md sm:max-w-sm pointer-events-none">
      {toasts.map((item) => (
        <div
          key={item.id}
          role="status"
          className="card px-4 py-2.5 text-sm flex items-center gap-3 animate-fade-up pointer-events-auto w-full"
          style={{
            background: 'var(--surface-2)',
            boxShadow: 'var(--shadow-lg)',
            borderColor:
              item.kind === 'error'
                ? 'color-mix(in oklab, var(--danger) 45%, transparent)'
                : item.kind === 'success'
                  ? 'color-mix(in oklab, var(--success) 45%, transparent)'
                  : 'var(--border)',
          }}
        >
          <span
            className="w-1.5 h-1.5 rounded-full shrink-0"
            style={{
              background:
                item.kind === 'error'
                  ? 'var(--danger)'
                  : item.kind === 'success'
                    ? 'var(--success)'
                    : 'var(--accent)',
            }}
          />
          <span className="flex-1 min-w-0">{item.message}</span>
          <button
            className="btn btn-ghost !p-1 shrink-0"
            onClick={() => dismiss(item.id)}
            aria-label="Dismiss"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>,
    document.body,
  )
}

/* -------------------------------------------------------------------------- */
/* Confirm dialog                                                              */
/* -------------------------------------------------------------------------- */

interface ConfirmOptions {
  title: string
  body?: ReactNode
  confirmLabel?: string
  danger?: boolean
}

const ConfirmContext = createContext<(options: ConfirmOptions) => Promise<boolean>>(
  async () => false,
)

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [options, setOptions] = useState<ConfirmOptions | null>(null)
  const resolver = useRef<((value: boolean) => void) | null>(null)

  const confirm = useCallback((next: ConfirmOptions) => {
    setOptions(next)
    return new Promise<boolean>((resolve) => {
      resolver.current = resolve
    })
  }, [])

  const settle = (value: boolean) => {
    resolver.current?.(value)
    resolver.current = null
    setOptions(null)
  }

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Modal
        open={options !== null}
        onClose={() => settle(false)}
        title={options?.title}
        width="sm"
        footer={
          <>
            <button className="btn btn-ghost" onClick={() => settle(false)}>
              Cancel
            </button>
            <button
              className={`btn ${options?.danger ? 'btn-danger' : 'btn-primary'}`}
              onClick={() => settle(true)}
              autoFocus
            >
              {options?.confirmLabel ?? 'Confirm'}
            </button>
          </>
        }
      >
        {options?.body && (
          <div className="px-5 py-4 text-sm" style={{ color: 'var(--text-muted)' }}>
            {options.body}
          </div>
        )}
      </Modal>
    </ConfirmContext.Provider>
  )
}

export const useConfirm = () => useContext(ConfirmContext)

/* -------------------------------------------------------------------------- */
/* Small primitives                                                            */
/* -------------------------------------------------------------------------- */

export function Switch({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean
  onChange: (value: boolean) => void
  label: ReactNode
  hint?: ReactNode
  disabled?: boolean
}) {
  return (
    <label
      className={`flex items-start gap-3 ${disabled ? 'opacity-50' : 'cursor-pointer'}`}
      onClick={(event) => {
        event.preventDefault()
        if (!disabled) onChange(!checked)
      }}
    >
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        className="relative shrink-0 rounded-full transition-colors mt-0.5"
        style={{
          width: 38,
          height: 22,
          background: checked ? 'var(--accent)' : 'var(--surface-3)',
        }}
      >
        <span
          className="absolute rounded-full bg-white transition-transform"
          style={{
            width: 16,
            height: 16,
            top: 3,
            left: 3,
            transform: checked ? 'translateX(16px)' : 'none',
            boxShadow: '0 1px 3px rgb(0 0 0 / 0.3)',
          }}
        />
      </button>
      <span className="min-w-0">
        <span className="text-sm font-medium block leading-snug">{label}</span>
        {hint && (
          <span className="text-xs block mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {hint}
          </span>
        )}
      </span>
    </label>
  )
}

export function ColorField({
  value,
  onChange,
  label,
}: {
  value: string
  onChange: (value: string) => void
  label?: string
}) {
  const presets = ['#5b6ee8', '#7c5cff', '#e8639b', '#f97362', '#f0a833', '#34d399', '#22b8cf', '#8b93a7']
  return (
    <div>
      {label && <span className="label">{label}</span>}
      <div className="flex items-center gap-2 flex-wrap">
        <label
          className="relative w-9 h-9 rounded-lg border cursor-pointer overflow-hidden shrink-0"
          style={{ background: value }}
        >
          <input
            type="color"
            value={value}
            onChange={(event) => onChange(event.target.value)}
            className="absolute inset-0 opacity-0 cursor-pointer"
          />
        </label>
        <input
          className="input font-mono !w-28 shrink-0"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          spellCheck={false}
        />
        <div className="flex gap-1.5 flex-wrap">
          {presets.map((preset) => (
            <button
              key={preset}
              type="button"
              onClick={() => onChange(preset)}
              className="w-6 h-6 rounded-md border transition-transform hover:scale-110"
              style={{
                background: preset,
                borderColor: value.toLowerCase() === preset ? 'var(--text)' : 'var(--border)',
              }}
              aria-label={`Use ${preset}`}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

export function Spinner({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      className="animate-spin"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="2.5" opacity="0.22" />
      <path
        d="M21 12a9 9 0 0 0-9-9"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
      />
    </svg>
  )
}

export function EmptyState({
  icon,
  title,
  body,
  action,
}: {
  icon?: ReactNode
  title: string
  body?: ReactNode
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center justify-center text-center px-6 py-12 gap-3">
      {icon && (
        <div
          className="w-14 h-14 rounded-2xl flex items-center justify-center"
          style={{ background: 'var(--surface-2)', color: 'var(--text-faint)' }}
        >
          {icon}
        </div>
      )}
      <div>
        <h3 className="font-semibold">{title}</h3>
        {body && (
          <p className="text-sm mt-1 max-w-sm" style={{ color: 'var(--text-muted)' }}>
            {body}
          </p>
        )}
      </div>
      {action}
    </div>
  )
}

export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: ReactNode
  hint?: ReactNode
  error?: string
  children: ReactNode
}) {
  return (
    <div>
      <span className="label">{label}</span>
      {children}
      {error ? (
        <p className="text-xs mt-1.5" style={{ color: 'var(--danger)' }}>
          {error}
        </p>
      ) : (
        hint && (
          <p className="text-xs mt-1.5" style={{ color: 'var(--text-faint)' }}>
            {hint}
          </p>
        )
      )}
    </div>
  )
}

export function Badge({
  children,
  color,
  title,
}: {
  children: ReactNode
  color?: string
  title?: string
}) {
  return (
    <span
      title={title}
      className="inline-flex items-center gap-1 text-[0.68rem] font-semibold px-1.5 py-0.5 rounded uppercase tracking-wide"
      style={{
        background: color
          ? `color-mix(in oklab, ${color} 20%, transparent)`
          : 'var(--accent-soft)',
        color: color ?? 'var(--accent)',
      }}
    >
      {children}
    </span>
  )
}

/**
 * The badge and icon of the highest-ranked role a member holds that defines
 * one. Roles carry visual identity now, not just a colour.
 */
export function RoleFlair({
  roles,
  memberRoleIds,
  size = 'sm',
}: {
  roles: import('../lib/types').Role[]
  memberRoleIds: string[]
  size?: 'xs' | 'sm'
}) {
  const ranked = roles
    .filter((role) => memberRoleIds.includes(role.id) && (role.badge || role.icon_url))
    .sort((a, b) => b.position - a.position)
  const flair = ranked[0]
  if (!flair) return null

  const pixels = size === 'xs' ? 12 : 14
  return (
    <span className="inline-flex items-center gap-1 shrink-0" title={flair.name}>
      {flair.icon_url && (
        <img
          src={flair.icon_url}
          alt=""
          className="object-contain rounded-sm"
          style={{ width: pixels, height: pixels }}
        />
      )}
      {flair.badge && (
        <span
          className="text-[0.62rem] font-bold px-1 rounded uppercase tracking-wide leading-[1.35]"
          style={{
            background: flair.color
              ? `color-mix(in oklab, ${flair.color} 22%, transparent)`
              : 'var(--surface-3)',
            color: flair.color ?? 'var(--text-muted)',
          }}
        >
          {flair.badge}
        </span>
      )}
    </span>
  )
}

/** Copy-to-clipboard with a graceful fallback for non-secure contexts. */
export async function copyText(value: string, successMessage = 'Copied to clipboard') {
  try {
    await navigator.clipboard.writeText(value)
    toast.success(successMessage)
  } catch {
    const field = document.createElement('textarea')
    field.value = value
    field.style.position = 'fixed'
    field.style.opacity = '0'
    document.body.appendChild(field)
    field.select()
    try {
      document.execCommand('copy')
      toast.success(successMessage)
    } catch {
      toast.error('Could not copy — copy it manually.')
    }
    document.body.removeChild(field)
  }
}
