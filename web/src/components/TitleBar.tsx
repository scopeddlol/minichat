import { useCallback, useEffect, useState } from 'react'
import { hasWindowControls, windowControls } from '../lib/desktop'
import { useStore } from '../lib/store'

/**
 * The desktop app's titlebar.
 *
 * The Tauri window is frameless, so this replaces the Windows caption bar
 * entirely: the instance's own name and icon on the left, the drag region
 * across the middle, and the three window buttons on the right. Nothing here
 * renders in a browser.
 *
 * `data-tauri-drag-region="deep"` is what Tauri's own handler looks for; it
 * treats clicks anywhere in the subtree as a drag and a double-click as
 * maximise, skipping anything interactive. That is why the buttons work
 * despite sitting inside the drag region.
 */
export default function TitleBar() {
  const meta = useStore((s) => s.meta)
  const [available] = useState(hasWindowControls)
  const [maximized, setMaximized] = useState(false)

  const sync = useCallback(async () => {
    const value = await windowControls.isMaximized()
    if (value !== null) setMaximized(value)
  }, [])

  // The window resizes when it is maximised or restored, from the titlebar or
  // from a keyboard shortcut or a snap gesture the page never sees. Watching
  // `resize` keeps the icon right without needing to listen for Tauri events,
  // which would mean granting the page a wider slice of IPC than this.
  useEffect(() => {
    if (!available) return
    void sync()
    window.addEventListener('resize', sync)
    return () => window.removeEventListener('resize', sync)
  }, [available, sync])

  if (!available) return null

  const name = meta?.name ?? 'MiniChat'

  return (
    <div
      data-tauri-drag-region="deep"
      className="fixed top-0 inset-x-0 z-[90] h-9 flex items-stretch select-none"
      style={{
        background: 'color-mix(in oklab, var(--surface-0) 82%, transparent)',
        backdropFilter: 'blur(14px)',
        borderBottom: '1px solid var(--border-soft)',
      }}
    >
      <div className="flex items-center gap-2 pl-3 min-w-0">
        {meta?.icon_url ? (
          <img src={meta.icon_url} alt="" className="w-4 h-4 rounded object-cover" />
        ) : (
          <svg width="15" height="15" viewBox="0 0 64 64" fill="none" aria-hidden="true">
            <path
              d="M22 16.6h20a8.4 8.4 0 0 1 8.4 8.4v10.6a8.4 8.4 0 0 1-8.4 8.4H30.5l-9.8 7v-7H22a8.4 8.4 0 0 1-8.4-8.4V25A8.4 8.4 0 0 1 22 16.6Z"
              fill="var(--accent)"
            />
          </svg>
        )}
        <span
          className="text-xs font-semibold tracking-wide truncate"
          style={{ color: 'var(--text-muted)' }}
        >
          {name}
        </span>
      </div>

      {/* Takes the remaining width so most of the bar is draggable. */}
      <div className="flex-1" data-tauri-drag-region="deep" />

      <div className="flex items-stretch">
        <CaptionButton label="Minimise" onClick={() => void windowControls.minimize()}>
          <path d="M1 5.5h9" />
        </CaptionButton>

        <CaptionButton
          label={maximized ? 'Restore' : 'Maximise'}
          onClick={() => void windowControls.toggleMaximize().then(sync)}
        >
          {maximized ? (
            <>
              <path d="M2.5 3.5h6v6h-6z" />
              <path d="M4 3.5V2h6v6H8.5" />
            </>
          ) : (
            <path d="M1.5 1.5h8v8h-8z" />
          )}
        </CaptionButton>

        <CaptionButton label="Close" danger onClick={() => void windowControls.close()}>
          <path d="M1.5 1.5l8 8M9.5 1.5l-8 8" />
        </CaptionButton>
      </div>
    </div>
  )
}

function CaptionButton({
  label,
  danger,
  onClick,
  children,
}: {
  label: string
  danger?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  const [hover, setHover] = useState(false)
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className="w-12 grid place-items-center transition-colors"
      style={{
        color: hover && danger ? '#fff' : hover ? 'var(--text)' : 'var(--text-faint)',
        background: hover
          ? danger
            ? '#e81123'
            : 'color-mix(in oklab, var(--text) 10%, transparent)'
          : 'transparent',
      }}
    >
      <svg
        width="11"
        height="11"
        viewBox="0 0 11 11"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
        aria-hidden="true"
      >
        {children}
      </svg>
    </button>
  )
}
