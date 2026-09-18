import {
  createContext, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState,
  type ReactNode,
} from 'react'
import { createPortal } from 'react-dom'

/**
 * Right-click menus.
 *
 * One menu element lives at the root and every caller opens it through
 * `useContextMenu()`, so there is a single source of truth for what is open,
 * a single Escape handler and a single outside-click handler. Opening a menu
 * while another is open replaces it rather than stacking.
 *
 * Positioning follows the pointer and flips at the viewport edge, the same
 * rule `<Select />` uses, because a menu that opens off-screen in the desktop
 * app is the exact class of bug the custom dropdown was built to avoid.
 *
 * On touch screens there is no right-click, so the same items are opened by a
 * long press and rendered as a bottom sheet.
 */

export interface MenuItem {
  /** A separator; every other field is ignored. */
  separator?: boolean
  label?: string
  icon?: ReactNode
  /** Right-aligned hint, for a shortcut like "Shift+Click". */
  hint?: string
  danger?: boolean
  disabled?: boolean
  onSelect?: () => void
}

interface MenuState {
  x: number
  y: number
  items: MenuItem[]
}

interface ContextMenuApi {
  /** Open at the event's position. Also calls preventDefault for you. */
  open: (event: { clientX: number; clientY: number; preventDefault: () => void }, items: MenuItem[]) => void
  close: () => void
}

const Ctx = createContext<ContextMenuApi | null>(null)

/** Everything a right-click menu needs, minus the menu itself. */
export function useContextMenu(): ContextMenuApi {
  const api = useContext(Ctx)
  if (!api) throw new Error('useContextMenu used outside <ContextMenuProvider>')
  return api
}

const SHEET_BREAKPOINT = 640
const MENU_WIDTH = 210

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<MenuState | null>(null)

  const open = useCallback<ContextMenuApi['open']>((event, items) => {
    event.preventDefault()
    // An empty menu would be a blank box; let the browser do nothing instead.
    if (items.length === 0) return
    setState({ x: event.clientX, y: event.clientY, items })
  }, [])
  const close = useCallback(() => setState(null), [])

  return (
    <Ctx.Provider value={{ open, close }}>
      {children}
      {state && <Menu state={state} onClose={close} />}
    </Ctx.Provider>
  )
}

function Menu({ state, onClose }: { state: MenuState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)
  const [active, setActive] = useState(-1)
  const sheet = typeof window !== 'undefined' && window.innerWidth < SHEET_BREAKPOINT

  const selectable = state.items
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => !item.separator && !item.disabled)

  // Measured, then placed: the menu's height depends on its items, and
  // guessing it puts the last item under the taskbar often enough to matter.
  useLayoutEffect(() => {
    if (sheet) return
    const height = ref.current?.offsetHeight ?? 0
    const width = ref.current?.offsetWidth ?? MENU_WIDTH
    const margin = 8
    setPos({
      left: Math.max(margin, Math.min(state.x, window.innerWidth - width - margin)),
      top: Math.max(margin, Math.min(state.y, window.innerHeight - height - margin)),
    })
  }, [state, sheet])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
        return
      }
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        const order = event.key === 'ArrowDown' ? selectable : [...selectable].reverse()
        const next = order.find(({ index }) =>
          event.key === 'ArrowDown' ? index > active : index < active,
        )
        setActive((next ?? order[0])?.index ?? -1)
        return
      }
      if (event.key === 'Enter' || event.key === ' ') {
        const chosen = state.items[active]
        if (chosen && !chosen.separator && !chosen.disabled) {
          event.preventDefault()
          onClose()
          chosen.onSelect?.()
        }
      }
    }
    // Capture, so Escape closes the menu before a dialog underneath sees it.
    document.addEventListener('keydown', onKey, true)
    // Any scroll or resize invalidates the anchor point entirely; a menu is
    // short-lived, so closing beats trying to follow.
    const onMove = () => onClose()
    window.addEventListener('scroll', onMove, true)
    window.addEventListener('resize', onMove)
    window.addEventListener('blur', onMove)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      window.removeEventListener('scroll', onMove, true)
      window.removeEventListener('resize', onMove)
      window.removeEventListener('blur', onMove)
    }
  }, [active, onClose, selectable, state.items])

  const items = state.items.map((item, index) =>
    item.separator ? (
      <div key={index} className="my-1 border-t" style={{ borderColor: 'var(--border-soft)' }} />
    ) : (
      <button
        key={index}
        type="button"
        role="menuitem"
        disabled={item.disabled}
        onMouseEnter={() => setActive(index)}
        onClick={() => {
          onClose()
          item.onSelect?.()
        }}
        className="w-full flex items-center gap-2.5 px-2.5 py-2 sm:py-1.5 rounded-lg text-sm text-left transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        style={{
          color: item.disabled
            ? 'var(--text-faint)'
            : item.danger
              ? 'var(--danger)'
              : 'var(--text)',
          background:
            active === index && !item.disabled
              ? item.danger
                ? 'color-mix(in oklab, var(--danger) 16%, transparent)'
                : 'var(--surface-2)'
              : 'transparent',
        }}
      >
        <span className="shrink-0 grid place-items-center w-4" style={{ opacity: 0.85 }}>
          {item.icon}
        </span>
        <span className="flex-1 truncate">{item.label}</span>
        {item.hint && (
          <span className="text-[0.68rem] shrink-0" style={{ color: 'var(--text-faint)' }}>
            {item.hint}
          </span>
        )}
      </button>
    ),
  )

  return createPortal(
    <>
      {/* Swallows the click that dismisses, so it can't also activate
          whatever sits underneath the menu. */}
      <div
        className="fixed inset-0 z-[84]"
        onMouseDown={(event) => {
          event.preventDefault()
          onClose()
        }}
        onContextMenu={(event) => {
          event.preventDefault()
          onClose()
        }}
      />
      <div
        ref={ref}
        role="menu"
        aria-orientation="vertical"
        className={
          sheet
            ? 'fixed z-[85] card p-1.5 left-2 right-2 bottom-2 animate-pop-in'
            : 'fixed z-[85] card p-1.5 animate-pop-in'
        }
        style={
          sheet
            ? {
                boxShadow: 'var(--shadow-lg)',
                paddingBottom: 'max(0.375rem, env(safe-area-inset-bottom))',
              }
            : {
                minWidth: MENU_WIDTH,
                maxWidth: 320,
                boxShadow: 'var(--shadow-lg)',
                left: pos?.left ?? state.x,
                top: pos?.top ?? state.y,
                // Hidden for the one frame between mount and measurement, so
                // the menu never visibly jumps into place.
                visibility: pos ? 'visible' : 'hidden',
              }
        }
      >
        {items}
      </div>
    </>,
    document.body,
  )
}

/**
 * Long-press handlers, for the touch screens that have no right-click.
 *
 * Returns props to spread onto the element that should open the menu. The
 * press is cancelled by movement so it doesn't fire while scrolling the
 * channel list or the message log.
 */
export function useLongPress(onTrigger: (point: { clientX: number; clientY: number }) => void) {
  const timer = useRef<number | null>(null)
  const start = useRef<{ x: number; y: number } | null>(null)

  const cancel = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current)
    timer.current = null
    start.current = null
  }, [])

  useEffect(() => cancel, [cancel])

  return {
    onTouchStart: (event: React.TouchEvent) => {
      const touch = event.touches[0]
      if (!touch) return
      start.current = { x: touch.clientX, y: touch.clientY }
      timer.current = window.setTimeout(() => {
        timer.current = null
        onTrigger({ clientX: touch.clientX, clientY: touch.clientY })
      }, 450)
    },
    onTouchMove: (event: React.TouchEvent) => {
      const touch = event.touches[0]
      if (!touch || !start.current) return
      const moved =
        Math.abs(touch.clientX - start.current.x) > 10 ||
        Math.abs(touch.clientY - start.current.y) > 10
      if (moved) cancel()
    },
    onTouchEnd: cancel,
    onTouchCancel: cancel,
  }
}
