/**
 * Voice hotkeys: push-to-talk, mute and deafen.
 *
 * In a browser these only fire while the app has focus — that's a platform
 * limit, not a choice. The desktop app registers the same bindings as OS-level
 * global shortcuts and forwards them in as `minichat:hotkey` events, so the
 * handler below is shared by both paths.
 */

export type HotkeyAction = 'ptt' | 'mute' | 'deafen'

export interface HotkeyBindings {
  ptt: string
  mute: string
  deafen: string
  /** Hold to talk, versus tap to toggle transmission. */
  pttMode: 'hold' | 'toggle'
  enabled: boolean
}

export const DEFAULT_BINDINGS: HotkeyBindings = {
  ptt: 'F8',
  mute: 'F9',
  deafen: 'F10',
  pttMode: 'hold',
  enabled: false,
}

const STORAGE_KEY = 'minichat.hotkeys'

export function loadBindings(): HotkeyBindings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_BINDINGS }
    return { ...DEFAULT_BINDINGS, ...(JSON.parse(raw) as Partial<HotkeyBindings>) }
  } catch {
    return { ...DEFAULT_BINDINGS }
  }
}

export function saveBindings(bindings: HotkeyBindings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(bindings))
  } catch {
    /* private browsing — bindings just won't persist */
  }
}

/** Turn a keyboard event into a stable, printable binding string. */
export function keyFromEvent(event: KeyboardEvent): string {
  const parts: string[] = []
  if (event.ctrlKey) parts.push('Ctrl')
  if (event.altKey) parts.push('Alt')
  if (event.shiftKey) parts.push('Shift')
  if (event.metaKey) parts.push('Meta')

  let key = event.key
  if (key === ' ') key = 'Space'
  // Ignore bare modifier presses; they can't be a binding on their own.
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(key)) return parts.join('+')
  if (key.length === 1) key = key.toUpperCase()
  parts.push(key)
  return parts.join('+')
}

export function isBindingComplete(binding: string): boolean {
  if (!binding) return false
  const last = binding.split('+').pop() ?? ''
  return !['Ctrl', 'Alt', 'Shift', 'Meta', ''].includes(last)
}

interface Handlers {
  onPushToTalk: (active: boolean) => void
  onToggleMute: () => void
  onToggleDeafen: () => void
}

/**
 * Attach the listeners. Returns a teardown function.
 *
 * Bindings are read through a getter rather than captured, so changing a key
 * in settings takes effect immediately without re-attaching.
 */
export function attachHotkeys(getBindings: () => HotkeyBindings, handlers: Handlers): () => void {
  let pttActive = false

  const isTyping = (target: EventTarget | null): boolean => {
    const element = target as HTMLElement | null
    if (!element) return false
    const tag = element.tagName
    return tag === 'INPUT' || tag === 'TEXTAREA' || element.isContentEditable
  }

  const onKeyDown = (event: KeyboardEvent) => {
    const bindings = getBindings()
    if (!bindings.enabled) return
    // A single printable key would otherwise swallow ordinary typing.
    if (isTyping(event.target) && !event.ctrlKey && !event.altKey && !event.metaKey) return

    const pressed = keyFromEvent(event)

    if (pressed === bindings.ptt) {
      event.preventDefault()
      if (bindings.pttMode === 'toggle') {
        if (event.repeat) return
        pttActive = !pttActive
        handlers.onPushToTalk(pttActive)
      } else if (!pttActive) {
        pttActive = true
        handlers.onPushToTalk(true)
      }
      return
    }
    if (pressed === bindings.mute && !event.repeat) {
      event.preventDefault()
      handlers.onToggleMute()
      return
    }
    if (pressed === bindings.deafen && !event.repeat) {
      event.preventDefault()
      handlers.onToggleDeafen()
    }
  }

  const onKeyUp = (event: KeyboardEvent) => {
    const bindings = getBindings()
    if (!bindings.enabled || bindings.pttMode !== 'hold') return
    // Compare on the bare key: releasing a modifier first would otherwise
    // leave the mic stuck open.
    const releasedKey = event.key === ' ' ? 'Space' : event.key
    const bound = bindings.ptt.split('+').pop()
    if (pttActive && (keyFromEvent(event) === bindings.ptt || releasedKey === bound)) {
      pttActive = false
      handlers.onPushToTalk(false)
    }
  }

  // Losing focus mid-hold must never leave the microphone live.
  const onBlur = () => {
    if (pttActive && getBindings().pttMode === 'hold') {
      pttActive = false
      handlers.onPushToTalk(false)
    }
  }

  // Forwarded from the desktop app's global shortcuts.
  const onDesktopHotkey = (event: Event) => {
    const detail = (event as CustomEvent).detail as { action: HotkeyAction; active?: boolean }
    if (!detail) return
    if (detail.action === 'ptt') {
      pttActive = Boolean(detail.active)
      handlers.onPushToTalk(pttActive)
    } else if (detail.action === 'mute') {
      handlers.onToggleMute()
    } else if (detail.action === 'deafen') {
      handlers.onToggleDeafen()
    }
  }

  window.addEventListener('keydown', onKeyDown)
  window.addEventListener('keyup', onKeyUp)
  window.addEventListener('blur', onBlur)
  window.addEventListener('minichat:hotkey', onDesktopHotkey)

  return () => {
    window.removeEventListener('keydown', onKeyDown)
    window.removeEventListener('keyup', onKeyUp)
    window.removeEventListener('blur', onBlur)
    window.removeEventListener('minichat:hotkey', onDesktopHotkey)
  }
}
