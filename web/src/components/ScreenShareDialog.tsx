import { Gauge, MonitorPlay, MonitorUp, Type, Volume2, AppWindow } from 'lucide-react'
import { FRAME_RATES, RESOLUTIONS, bitrateFor } from '../lib/screenshare'
import { useVoice } from '../lib/voice'
import { Modal, Switch, toast } from './ui'

/**
 * Quality chosen before the share starts.
 *
 * The browser's source picker opens after this, which is why the copy says so
 * — being told what's about to happen is better than being surprised by a
 * dialog you didn't ask for.
 */
export default function ScreenShareDialog({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const settings = useVoice((s) => s.screenShare)
  const update = useVoice((s) => s.setScreenShareSettings)
  const start = useVoice((s) => s.startScreenShare)

  const estimate = Math.round(bitrateFor(settings.resolution, settings.fps) / 100_000) / 10

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Share your screen"
      description="Pick a quality, then choose what to share."
      width="sm"
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={async () => {
              onClose()
              try {
                await start()
              } catch {
                toast.error('Could not start sharing your screen.')
              }
            }}
          >
            <MonitorUp size={15} /> Choose what to share
          </button>
        </>
      }
    >
      <div className="px-5 py-4 space-y-5">
        <div>
          <span className="label">Resolution</span>
          <div className="grid grid-cols-3 sm:grid-cols-5 gap-1.5">
            {RESOLUTIONS.map((option) => (
              <button
                key={option.value}
                onClick={() => update({ resolution: option.value })}
                title={option.hint}
                className="py-2 rounded-lg border text-xs font-semibold transition-colors"
                style={{
                  borderColor: settings.resolution === option.value ? 'var(--accent)' : 'var(--border)',
                  background: settings.resolution === option.value ? 'var(--accent-soft)' : 'transparent',
                  color: settings.resolution === option.value ? 'var(--accent)' : 'var(--text-muted)',
                }}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="label">Frame rate</span>
          <div className="grid grid-cols-3 gap-1.5">
            {FRAME_RATES.map((option) => (
              <button
                key={option.value}
                onClick={() => update({ fps: option.value })}
                title={option.hint}
                className="py-2 rounded-lg border text-xs font-semibold transition-colors"
                style={{
                  borderColor: settings.fps === option.value ? 'var(--accent)' : 'var(--border)',
                  background: settings.fps === option.value ? 'var(--accent-soft)' : 'transparent',
                  color: settings.fps === option.value ? 'var(--accent)' : 'var(--text-muted)',
                }}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="label">Optimise for</span>
          <div className="grid grid-cols-2 gap-1.5">
            {(
              [
                ['motion', 'Motion', 'Games and video — keeps frames smooth', <MonitorPlay size={14} key="m" />],
                ['detail', 'Detail', 'Code and text — keeps the picture sharp', <Type size={14} key="d" />],
              ] as const
            ).map(([value, label, hint, icon]) => (
              <button
                key={value}
                onClick={() => update({ content: value })}
                className="flex flex-col items-start gap-1 p-2.5 rounded-lg border text-left transition-colors"
                style={{
                  borderColor: settings.content === value ? 'var(--accent)' : 'var(--border)',
                  background: settings.content === value ? 'var(--accent-soft)' : 'transparent',
                }}
              >
                <span
                  className="flex items-center gap-1.5 text-xs font-semibold"
                  style={{ color: settings.content === value ? 'var(--accent)' : 'var(--text)' }}
                >
                  {icon} {label}
                </span>
                <span className="text-[0.68rem] leading-snug" style={{ color: 'var(--text-muted)' }}>
                  {hint}
                </span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="label">Start the picker on</span>
          <div className="grid grid-cols-2 gap-1.5">
            {(
              [
                ['monitor', 'Whole screen', <MonitorPlay size={14} key="s" />],
                ['window', 'A single window', <AppWindow size={14} key="w" />],
              ] as const
            ).map(([value, label, icon]) => (
              <button
                key={value}
                onClick={() => update({ source: value })}
                className="flex items-center justify-center gap-1.5 py-2 rounded-lg border text-xs font-semibold transition-colors"
                style={{
                  borderColor: settings.source === value ? 'var(--accent)' : 'var(--border)',
                  background: settings.source === value ? 'var(--accent-soft)' : 'transparent',
                  color: settings.source === value ? 'var(--accent)' : 'var(--text-muted)',
                }}
              >
                {icon} {label}
              </button>
            ))}
          </div>
        </div>

        <Switch
          checked={settings.audio}
          onChange={(value) => update({ audio: value })}
          label={
            <span className="flex items-center gap-1.5">
              <Volume2 size={14} /> Share audio too
            </span>
          }
          hint="Where the browser supports it — Chrome and Edge do, Firefox and Safari mostly don't."
        />

        <p
          className="text-xs flex items-start gap-2 p-2.5 rounded-lg"
          style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
        >
          <Gauge size={14} className="shrink-0 mt-0.5" />
          <span>
            About <strong>{estimate} Mbps</strong> up at this setting. Your browser will ask which
            screen or window to share next — that prompt is the browser's own and can't be skipped.
          </span>
        </p>
      </div>
    </Modal>
  )
}
