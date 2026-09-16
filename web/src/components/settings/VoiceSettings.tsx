import { Headphones, Keyboard, Mic, Monitor, RefreshCw, Video, Volume2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import {
  DEFAULT_BINDINGS, isBindingComplete, keyFromEvent, loadBindings, saveBindings,
  type HotkeyBindings,
} from '../../lib/hotkeys'
import { useStore } from '../../lib/store'
import { useVoice, type DeviceSelection } from '../../lib/voice'
import { Avatar, Field, Switch, toast } from '../ui'

export default function VoiceSettings() {
  const voice = useVoice()
  const members = useStore((s) => s.members)
  const voiceEnabled = useStore((s) => s.voiceEnabled)
  const [bindings, setBindings] = useState<HotkeyBindings>(loadBindings)
  const [capturing, setCapturing] = useState<keyof HotkeyBindings | null>(null)

  useEffect(() => {
    void voice.refreshDevices()
    // Only on mount; the store action is stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Capture the next key press as a binding.
  useEffect(() => {
    if (!capturing) return
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopPropagation()
      if (event.key === 'Escape') {
        setCapturing(null)
        return
      }
      const binding = keyFromEvent(event)
      if (!isBindingComplete(binding)) return
      update({ [capturing]: binding } as Partial<HotkeyBindings>)
      setCapturing(null)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [capturing])

  const update = (patch: Partial<HotkeyBindings>) => {
    const next = { ...bindings, ...patch }
    setBindings(next)
    saveBindings(next)
  }

  const isDesktop = navigator.userAgent.includes('MiniChat')

  const voiceMembers = Object.values(members).filter((member) => member.id !== useStore.getState().me?.id)

  return (
    <div className="space-y-6 animate-fade-in">
      <h2 className="text-lg font-semibold">Voice &amp; video</h2>

      {!voiceEnabled && (
        <p
          className="text-xs p-3 rounded-lg leading-relaxed"
          style={{ background: 'color-mix(in oklab, var(--warning) 12%, transparent)', color: 'var(--warning)' }}
        >
          Voice isn't configured on this instance yet, so these settings won't do anything until the
          operator sets up LiveKit.
        </p>
      )}

      {/* Devices */}
      <section className="space-y-4">
        <div className="flex items-center justify-between">
          <span className="label !mb-0">Devices</span>
          <button className="btn btn-ghost !py-1 !px-2 !text-xs" onClick={() => void voice.refreshDevices()}>
            <RefreshCw size={12} /> Refresh
          </button>
        </div>

        <DevicePicker
          icon={<Mic size={15} />}
          label="Microphone"
          kind="audioinput"
          options={voice.devices.audioinput}
          value={voice.selectedDevices.audioinput}
          onChange={(id) => void voice.selectDevice('audioinput', id)}
        />
        <DevicePicker
          icon={<Headphones size={15} />}
          label="Output"
          kind="audiooutput"
          options={voice.devices.audiooutput}
          value={voice.selectedDevices.audiooutput}
          onChange={(id) => void voice.selectDevice('audiooutput', id)}
          hint={
            voice.devices.audiooutput.length === 0
              ? 'Your browser does not allow choosing an output device — it follows the system default.'
              : undefined
          }
        />
        <DevicePicker
          icon={<Video size={15} />}
          label="Camera"
          kind="videoinput"
          options={voice.devices.videoinput}
          value={voice.selectedDevices.videoinput}
          onChange={(id) => void voice.selectDevice('videoinput', id)}
        />

        {voice.devices.audioinput.every((device) => !device.label) && (
          <p className="text-xs" style={{ color: 'var(--text-faint)' }}>
            Device names appear once you've joined a voice channel and granted microphone access.
          </p>
        )}
      </section>

      {/* Hotkeys */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Keyboard size={15} style={{ color: 'var(--accent)' }} />
          <span className="label !mb-0">Hotkeys</span>
        </div>

        <Switch
          checked={bindings.enabled}
          onChange={(value) => update({ enabled: value })}
          label="Enable voice hotkeys"
          hint={
            isDesktop
              ? 'In the desktop app these also work while MiniChat is in the background.'
              : 'In a browser these only work while the MiniChat window is focused.'
          }
        />

        {bindings.enabled && (
          <>
            <Field label="Push-to-talk mode">
              <div className="grid grid-cols-2 gap-2">
                {(
                  [
                    ['hold', 'Hold to talk'],
                    ['toggle', 'Tap to toggle'],
                  ] as const
                ).map(([value, label]) => (
                  <button
                    key={value}
                    onClick={() => update({ pttMode: value })}
                    className="py-2 rounded-lg border text-sm font-medium transition-colors"
                    style={{
                      borderColor: bindings.pttMode === value ? 'var(--accent)' : 'var(--border)',
                      background: bindings.pttMode === value ? 'var(--accent-soft)' : 'transparent',
                      color: bindings.pttMode === value ? 'var(--accent)' : 'var(--text-muted)',
                    }}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </Field>

            <div className="space-y-2">
              <BindingRow
                label="Push to talk"
                binding={bindings.ptt}
                capturing={capturing === 'ptt'}
                onCapture={() => setCapturing('ptt')}
                onReset={() => update({ ptt: DEFAULT_BINDINGS.ptt })}
              />
              <BindingRow
                label="Toggle mute"
                binding={bindings.mute}
                capturing={capturing === 'mute'}
                onCapture={() => setCapturing('mute')}
                onReset={() => update({ mute: DEFAULT_BINDINGS.mute })}
              />
              <BindingRow
                label="Toggle deafen"
                binding={bindings.deafen}
                capturing={capturing === 'deafen'}
                onCapture={() => setCapturing('deafen')}
                onReset={() => update({ deafen: DEFAULT_BINDINGS.deafen })}
              />
            </div>

            {!isDesktop && (
              <p
                className="text-xs p-2.5 rounded-lg leading-relaxed flex gap-2"
                style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
              >
                <Monitor size={14} className="shrink-0 mt-0.5" />
                <span>
                  Browsers can't listen for keys while another window is focused. Install the desktop
                  app for global push-to-talk.
                </span>
              </p>
            )}
          </>
        )}
      </section>

      {/* Per-user volume */}
      <section>
        <div className="flex items-center gap-2 mb-2">
          <Volume2 size={15} style={{ color: 'var(--accent)' }} />
          <span className="label !mb-0">Member volume</span>
        </div>
        <p className="text-xs mb-3" style={{ color: 'var(--text-muted)' }}>
          Only affects what you hear. Saved on this device.
        </p>

        <div className="card divide-y overflow-hidden max-h-64 overflow-y-auto scroll-thin">
          {voiceMembers.map((member) => {
            const volume = voice.volumes[member.id] ?? 1
            return (
              <div key={member.id} className="flex items-center gap-3 px-3 py-2">
                <Avatar
                  id={member.id}
                  name={member.display_name}
                  src={member.avatar_url}
                  accent={member.accent_color}
                  size="sm"
                />
                <span className="text-sm flex-1 min-w-0 truncate">{member.display_name}</span>
                <input
                  type="range"
                  min={0}
                  max={2}
                  step={0.05}
                  value={volume}
                  onChange={(event) => voice.setUserVolume(member.id, Number(event.target.value))}
                  className="w-28 accent-[var(--accent)]"
                  aria-label={`Volume for ${member.display_name}`}
                />
                <span
                  className="text-xs tabular-nums w-10 text-right"
                  style={{ color: volume === 0 ? 'var(--danger)' : 'var(--text-faint)' }}
                >
                  {volume === 0 ? 'muted' : `${Math.round(volume * 100)}%`}
                </span>
              </div>
            )
          })}
          {!voiceMembers.length && (
            <p className="text-sm px-3 py-6 text-center" style={{ color: 'var(--text-faint)' }}>
              No other members yet.
            </p>
          )}
        </div>
      </section>
    </div>
  )
}

function DevicePicker({
  icon,
  label,
  kind,
  options,
  value,
  onChange,
  hint,
}: {
  icon: React.ReactNode
  label: string
  kind: keyof DeviceSelection
  options: { deviceId: string; label: string }[]
  value: string
  onChange: (deviceId: string) => void
  hint?: string
}) {
  return (
    <Field
      label={
        <span className="flex items-center gap-1.5">
          {icon} {label}
        </span>
      }
      hint={hint}
    >
      <select
        className="input"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={options.length === 0}
        aria-label={label}
      >
        <option value="default">System default</option>
        {options
          .filter((option) => option.deviceId !== 'default')
          .map((option) => (
            <option key={`${kind}-${option.deviceId}`} value={option.deviceId}>
              {option.label}
            </option>
          ))}
      </select>
    </Field>
  )
}

function BindingRow({
  label,
  binding,
  capturing,
  onCapture,
  onReset,
}: {
  label: string
  binding: string
  capturing: boolean
  onCapture: () => void
  onReset: () => void
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 rounded-lg" style={{ background: 'var(--surface-2)' }}>
      <span className="text-sm flex-1">{label}</span>
      <button
        className="btn btn-subtle !py-1 !px-3 font-mono !text-xs min-w-24"
        onClick={onCapture}
        style={capturing ? { borderColor: 'var(--accent)', color: 'var(--accent)' } : undefined}
      >
        {capturing ? 'Press a key…' : binding}
      </button>
      <button
        className="btn btn-ghost !p-1.5"
        onClick={() => {
          onReset()
          toast.info(`${label} reset to its default.`)
        }}
        title="Reset"
      >
        <RefreshCw size={13} />
      </button>
    </div>
  )
}
