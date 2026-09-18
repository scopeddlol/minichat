import { Check, ChevronDown, Headphones, Mic, MicOff, VolumeX } from 'lucide-react'
import { useEffect } from 'react'
import { useVoice } from '../lib/voice'
import { useContextMenu } from './ContextMenu'
import { toast } from './ui'

export default function VoiceDeviceControl({ kind }: { kind: 'audioinput' | 'audiooutput' }) {
  const voice = useVoice()
  const menu = useContextMenu()
  const input = kind === 'audioinput'
  const off = input ? voice.muted : voice.deafened
  const label = input ? 'Microphone' : 'Speakers'
  useEffect(() => {
    const refresh = () => {
      void useVoice.getState().refreshDevices()
    }
    refresh()
    navigator.mediaDevices?.addEventListener('devicechange', refresh)
    return () => navigator.mediaDevices?.removeEventListener('devicechange', refresh)
  }, [])
  return (
    <div className="flex min-w-0 rounded-lg" style={{ background: 'var(--surface-2)' }}>
      <button
        className="flex-1 min-w-0 flex items-center justify-center py-2 rounded-l-lg"
        disabled={input && !voice.canSpeak}
        style={{ color: off ? 'var(--danger)' : 'var(--text-muted)' }}
        aria-label={input ? (off ? 'Unmute' : 'Mute') : off ? 'Undeafen' : 'Deafen'}
        aria-pressed={off}
        onClick={() => void (input ? voice.toggleMute() : voice.toggleDeafen())}
      >
        {input ? (
          off ? (
            <MicOff size={15} />
          ) : (
            <Mic size={15} />
          )
        ) : off ? (
          <VolumeX size={15} />
        ) : (
          <Headphones size={15} />
        )}
      </button>
      <button
        className="px-1 py-2 rounded-r-lg hover:bg-[var(--surface-3)]"
        aria-label={`${label} devices`}
        aria-haspopup="menu"
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect()
          const devices = voice.devices[kind]
          menu.open(
            { clientX: rect.left, clientY: rect.top, preventDefault() {} },
            [
              { deviceId: 'default', label: 'System default' },
              ...devices.filter((d) => d.deviceId !== 'default'),
            ].map((device) => ({
              label: device.label,
              icon:
                voice.selectedDevices[kind] === device.deviceId ? <Check size={14} /> : undefined,
              onSelect: () => {
                void voice.selectDevice(kind, device.deviceId).catch((e) => toast.error(e.message))
              },
            })),
          )
          void voice.refreshDevices()
        }}
      >
        <ChevronDown size={12} />
      </button>
    </div>
  )
}
