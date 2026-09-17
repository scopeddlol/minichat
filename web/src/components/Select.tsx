import { Check, ChevronDown } from 'lucide-react'
import {
  useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState,
  type ReactNode,
} from 'react'
import { createPortal } from 'react-dom'

/**
 * A dropdown rendered in the DOM rather than by the OS.
 *
 * WebView2 draws a native `<select>` as a real Win32 combo box, which on
 * Windows lands in the wrong place, clips against the window edge, ignores the
 * page's scroll position and cannot be styled. On a phone the native picker is
 * usable but cramped and unthemed. One component solves both: a listbox on
 * pointer-sized screens, a bottom sheet on touch-sized ones.
 */

export interface SelectOption<T extends string = string> {
  value: T
  label: string
  /** A second line, for options that need explaining. */
  hint?: string
  icon?: ReactNode
  /** A small colour swatch, used by the role and channel pickers. */
  swatch?: string | null
  disabled?: boolean
}

interface Props<T extends string> {
  value: T
  onChange: (value: T) => void
  options: SelectOption<T>[]
  placeholder?: string
  disabled?: boolean
  /** Accessible name when there's no visible <label>. */
  ariaLabel?: string
  className?: string
  /** Renders at input width by default; `auto` hugs its content. */
  width?: 'full' | 'auto'
}

const SHEET_BREAKPOINT = 640

export default function Select<T extends string>({
  value,
  onChange,
  options,
  placeholder = 'Select…',
  disabled,
  ariaLabel,
  className = '',
  width = 'full',
}: Props<T>) {
  const listboxId = useId()
  const trigger = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [rect, setRect] = useState<DOMRect | null>(null)
  const [asSheet, setAsSheet] = useState(false)
  const typeAhead = useRef({ query: '', at: 0 })

  const selected = useMemo(
    () => options.find((option) => option.value === value) ?? null,
    [options, value],
  )

  const openList = useCallback(() => {
    if (disabled) return
    const element = trigger.current
    if (!element) return
    setRect(element.getBoundingClientRect())
    setAsSheet(window.innerWidth < SHEET_BREAKPOINT)
    setActive(Math.max(0, options.findIndex((option) => option.value === value)))
    setOpen(true)
  }, [disabled, options, value])

  const close = useCallback(() => {
    setOpen(false)
    trigger.current?.focus()
  }, [])

  const commit = useCallback(
    (index: number) => {
      const option = options[index]
      if (!option || option.disabled) return
      onChange(option.value)
      setOpen(false)
      trigger.current?.focus()
    },
    [onChange, options],
  )

  // Follow the trigger while open: a scroll or resize underneath the list
  // would otherwise leave it stranded — the same bug the native control has.
  useEffect(() => {
    if (!open) return
    const reposition = () => {
      const element = trigger.current
      if (!element) return
      setRect(element.getBoundingClientRect())
    }
    window.addEventListener('scroll', reposition, true)
    window.addEventListener('resize', reposition)
    return () => {
      window.removeEventListener('scroll', reposition, true)
      window.removeEventListener('resize', reposition)
    }
  }, [open])

  useLayoutEffect(() => {
    if (!open) return
    listRef.current
      ?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: 'nearest' })
  }, [open, active])

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!open) {
      if (['Enter', ' ', 'ArrowDown', 'ArrowUp'].includes(event.key)) {
        event.preventDefault()
        openList()
      }
      return
    }

    switch (event.key) {
      case 'Escape':
        event.preventDefault()
        close()
        break
      case 'Tab':
        setOpen(false)
        break
      case 'Enter':
      case ' ':
        event.preventDefault()
        commit(active)
        break
      case 'ArrowDown':
        event.preventDefault()
        setActive((index) => nextEnabled(options, index, 1))
        break
      case 'ArrowUp':
        event.preventDefault()
        setActive((index) => nextEnabled(options, index, -1))
        break
      case 'Home':
        event.preventDefault()
        setActive(nextEnabled(options, -1, 1))
        break
      case 'End':
        event.preventDefault()
        setActive(nextEnabled(options, options.length, -1))
        break
      default: {
        // Type-ahead, the way a native select behaves.
        if (event.key.length !== 1 || event.metaKey || event.ctrlKey || event.altKey) return
        const now = Date.now()
        const state = typeAhead.current
        state.query = now - state.at > 900 ? event.key : state.query + event.key
        state.at = now
        const query = state.query.toLowerCase()
        const found = options.findIndex(
          (option) => !option.disabled && option.label.toLowerCase().startsWith(query),
        )
        if (found >= 0) setActive(found)
      }
    }
  }

  const list = open && rect && (
    <>
      <div
        className="fixed inset-0 z-[80]"
        style={asSheet ? { background: 'rgb(0 0 0 / 0.5)' } : undefined}
        onMouseDown={() => setOpen(false)}
      />
      <div
        ref={listRef}
        role="listbox"
        id={listboxId}
        aria-activedescendant={`${listboxId}-${active}`}
        className={`fixed z-[81] card overflow-y-auto scroll-thin p-1 ${
          asSheet ? 'animate-fade-up' : 'animate-pop-in'
        }`}
        style={
          asSheet
            ? {
                // A bottom sheet: reachable with a thumb, and it can never be
                // pushed off-screen by a small viewport.
                left: 12,
                right: 12,
                bottom: 12,
                maxHeight: '60vh',
                boxShadow: 'var(--shadow-lg)',
                paddingBottom: 'max(0.25rem, env(safe-area-inset-bottom))',
              }
            : {
                left: Math.max(8, Math.min(rect.left, window.innerWidth - rect.width - 8)),
                width: Math.max(rect.width, 180),
                // Flip above the trigger when there isn't room below.
                ...(window.innerHeight - rect.bottom < 240 && rect.top > 260
                  ? { bottom: window.innerHeight - rect.top + 6, maxHeight: rect.top - 16 }
                  : { top: rect.bottom + 6, maxHeight: window.innerHeight - rect.bottom - 24 }),
                boxShadow: 'var(--shadow-lg)',
              }
        }
      >
        {options.map((option, index) => {
          const isSelected = option.value === value
          return (
            <button
              key={option.value}
              id={`${listboxId}-${index}`}
              role="option"
              aria-selected={isSelected}
              data-active={index === active}
              disabled={option.disabled}
              type="button"
              onMouseEnter={() => !option.disabled && setActive(index)}
              onMouseDown={(event) => {
                event.preventDefault()
                commit(index)
              }}
              className="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-left transition-colors disabled:opacity-40"
              style={{
                background: index === active && !option.disabled ? 'var(--surface-2)' : 'transparent',
                minHeight: asSheet ? 44 : undefined,
              }}
            >
              {option.swatch !== undefined && (
                <span
                  className="w-2.5 h-2.5 rounded-full shrink-0"
                  style={{ background: option.swatch ?? 'var(--text-faint)' }}
                />
              )}
              {option.icon}
              <span className="min-w-0 flex-1">
                <span className="text-sm block truncate">{option.label}</span>
                {option.hint && (
                  <span className="text-xs block truncate" style={{ color: 'var(--text-muted)' }}>
                    {option.hint}
                  </span>
                )}
              </span>
              {isSelected && (
                <Check size={15} className="shrink-0" style={{ color: 'var(--accent)' }} />
              )}
            </button>
          )
        })}
        {!options.length && (
          <p className="text-sm px-2.5 py-3" style={{ color: 'var(--text-faint)' }}>
            Nothing to choose from.
          </p>
        )}
      </div>
    </>
  )

  return (
    <>
      <button
        ref={trigger}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : openList())}
        onKeyDown={onKeyDown}
        className={`input flex items-center gap-2 text-left ${
          width === 'auto' ? '!w-auto' : ''
        } ${className}`}
        style={{ borderColor: open ? 'var(--accent)' : undefined }}
      >
        {selected?.swatch !== undefined && selected && (
          <span
            className="w-2.5 h-2.5 rounded-full shrink-0"
            style={{ background: selected.swatch ?? 'var(--text-faint)' }}
          />
        )}
        {selected?.icon}
        <span
          className="flex-1 min-w-0 truncate"
          style={{ color: selected ? undefined : 'var(--text-faint)' }}
        >
          {selected?.label ?? placeholder}
        </span>
        <ChevronDown
          size={15}
          className="shrink-0 transition-transform"
          style={{ color: 'var(--text-faint)', transform: open ? 'rotate(180deg)' : undefined }}
        />
      </button>
      {open && createPortal(list, document.body)}
    </>
  )
}

/** Next selectable index in `direction`, wrapping at the ends. */
function nextEnabled<T extends string>(
  options: SelectOption<T>[],
  from: number,
  direction: 1 | -1,
): number {
  if (!options.length) return 0
  let index = from
  for (let step = 0; step < options.length; step++) {
    index = (index + direction + options.length) % options.length
    if (!options[index]?.disabled) return index
  }
  return Math.max(0, from)
}
