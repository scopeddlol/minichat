import { Maximize2, Move, RotateCcw } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import type { ImageFrame } from '../lib/types'

export const CENTRED: ImageFrame = { x: 50, y: 50, zoom: 1 }

/**
 * Drag to reposition, slide to zoom.
 *
 * Nothing is rendered to a canvas and nothing is re-uploaded: the result is
 * three numbers the server stores and every avatar applies through
 * `frameStyle`. That keeps the original image intact, makes re-framing free,
 * and means an existing upload can be re-cropped later without the file.
 *
 * The preview is the real thing — the same `object-position` and `scale` the
 * rest of the app uses — rather than an approximation of it.
 */
export default function ImageFramer({
  src,
  frame,
  onChange,
  shape = 'circle',
  aspect = 1,
}: {
  src: string
  frame: ImageFrame
  onChange: (next: ImageFrame) => void
  shape?: 'circle' | 'banner'
  /** Width / height of the preview, for banners that aren't square. */
  aspect?: number
}) {
  const box = useRef<HTMLDivElement>(null)
  const [dragging, setDragging] = useState(false)

  // Kept in a ref so the window listeners below never close over a stale frame.
  const latest = useRef(frame)
  latest.current = frame

  const move = useCallback(
    (event: PointerEvent | React.PointerEvent) => {
      const element = box.current
      if (!element) return
      const rect = element.getBoundingClientRect()
      // Position is a percentage of the image, so a drag has to be scaled by
      // the preview's size or it moves at the wrong rate on a small preview.
      const x = ((event.clientX - rect.left) / rect.width) * 100
      const y = ((event.clientY - rect.top) / rect.height) * 100
      onChange({
        ...latest.current,
        x: Math.min(100, Math.max(0, x)),
        y: Math.min(100, Math.max(0, y)),
      })
    },
    [onChange],
  )

  // Listeners on the window, not the element: a drag that leaves the preview
  // should keep tracking until the button comes up, the way every other
  // drag-to-position control behaves.
  useEffect(() => {
    if (!dragging) return
    const onMove = (event: PointerEvent) => move(event)
    const stop = () => setDragging(false)
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', stop)
    window.addEventListener('pointercancel', stop)
    return () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', stop)
      window.removeEventListener('pointercancel', stop)
    }
  }, [dragging, move])

  const isDefault = frame.x === 50 && frame.y === 50 && frame.zoom === 1

  return (
    <div className="space-y-2.5">
      <div
        ref={box}
        onPointerDown={(event) => {
          event.preventDefault()
          setDragging(true)
          move(event)
        }}
        className={`relative overflow-hidden border select-none touch-none ${
          shape === 'circle' ? 'rounded-full mx-auto' : 'rounded-xl'
        }`}
        style={{
          width: shape === 'circle' ? 128 : '100%',
          aspectRatio: shape === 'circle' ? '1' : String(aspect),
          cursor: dragging ? 'grabbing' : 'grab',
          borderColor: 'var(--border)',
          background: 'var(--surface-2)',
        }}
      >
        <img
          src={src}
          alt=""
          draggable={false}
          className="w-full h-full object-cover pointer-events-none"
          style={{
            objectPosition: `${frame.x}% ${frame.y}%`,
            transform: frame.zoom === 1 ? undefined : `scale(${frame.zoom})`,
            transformOrigin: `${frame.x}% ${frame.y}%`,
          }}
        />
        {!dragging && (
          <div
            className="absolute inset-0 grid place-items-center pointer-events-none opacity-0 hover:opacity-100 transition-opacity"
            style={{ background: 'rgb(0 0 0 / 0.35)' }}
          >
            <Move size={18} style={{ color: '#fff' }} />
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <Maximize2 size={13} className="shrink-0" style={{ color: 'var(--text-faint)' }} />
        <input
          type="range"
          min={1}
          max={4}
          step={0.05}
          value={frame.zoom}
          aria-label="Zoom"
          className="flex-1 accent-[var(--accent)]"
          onChange={(event) => onChange({ ...frame, zoom: Number(event.target.value) })}
        />
        <span
          className="text-[0.68rem] tabular-nums w-8 text-right shrink-0"
          style={{ color: 'var(--text-faint)' }}
        >
          {frame.zoom.toFixed(1)}×
        </span>
        <button
          className="btn btn-ghost !p-1.5 shrink-0"
          onClick={() => onChange({ ...CENTRED })}
          disabled={isDefault}
          title="Reset framing"
          aria-label="Reset framing"
        >
          <RotateCcw size={13} />
        </button>
      </div>

      <p className="text-[0.68rem] text-center" style={{ color: 'var(--text-faint)' }}>
        Drag the image to choose what shows.
      </p>
    </div>
  )
}
