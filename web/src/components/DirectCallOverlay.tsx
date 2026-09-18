import VoiceDeviceControl from './VoiceDeviceControl'
import { useEffect, useState } from 'react'
import { Phone, PhoneOff, Video, ScreenShare } from 'lucide-react'
import { direct, useInbox, type DirectCall } from '../lib/direct'
import { gateway } from '../lib/gateway'
import { useStore } from '../lib/store'
import { useVoice } from '../lib/voice'
import VoiceStage from './VoiceStage'
import { toast } from './ui'

export default function DirectCallOverlay({ onShare }: { onShare: () => void }) {
  const [calls, setCalls] = useState<DirectCall[]>([])
  const [now, setNow] = useState(Date.now())
  const me = useStore((s) => s.me)
  const members = useStore((s) => s.members)
  const conversations = useInbox((s) => s.conversations)
  const voice = useVoice()
  useEffect(() => {
    let disposed = false
    let revision = 0
    const refresh = () => {
      const started = ++revision
      return direct
        .calls()
        .then((c) => {
          if (!disposed && started === revision) setCalls(c)
        })
        .catch(() => undefined)
    }
    void refresh()
    const timer = setInterval(() => {
      setNow(Date.now())
      void refresh()
    }, 5000)
    const off = gateway.on((event) => {
      if (event.t === 'READY') void refresh()
      if (event.t !== 'DIRECT_CALL') return
      ++revision
      const call = event.d as DirectCall
      setCalls((old) => [...old.filter((c) => c.id !== call.id), ...(call.status === 'ended' ? [] : [call])])
      if (call.status === 'ended' && useVoice.getState().channelId === `direct:${call.id}`)
        void useVoice.getState().leave()
      if (call.status === 'accepted' && call.caller_id === useStore.getState().me?.id) {
        void useVoice
          .getState()
          .join(`direct:${call.id}`)
          .catch((e) => {
            toast.error(e.message)
            void direct.action(call.id, 'end').catch(() => undefined)
          })
      }
    })
    return () => {
      disposed = true
      clearInterval(timer)
      off()
    }
  }, [])
  useEffect(() => {
    if (!voice.channelId?.startsWith('direct:') || !voice.connected) return
    if (!calls.some((c) => `direct:${c.id}` === voice.channelId && c.status === 'accepted'))
      void voice.leave()
  }, [calls, voice.channelId, voice.connected, voice.leave])
  const active = calls.find((c) => voice.channelId === `direct:${c.id}`)
  const ringing = calls.find(
    (c) => c.status === 'ringing' && now - new Date(c.created_at.replace(' ', 'T') + 'Z').getTime() < 45000,
  )
  const call = active ?? ringing ?? calls.find((c) => c.status === 'accepted')
  if (!call) return null
  const peerId = conversations.find((c) => c.id === call.conversation_id)?.peer_id ?? call.caller_id
  const incoming = call.caller_id !== me?.id
  const connected = voice.channelId === `direct:${call.id}` && voice.connected
  const join = () =>
    void (async () => {
      try {
        if (call.status === 'ringing') await direct.action(call.id, 'accept')
        await voice.join(`direct:${call.id}`)
      } catch (e) {
        toast.error(e instanceof Error ? e.message : 'Could not connect')
        await direct.action(call.id, 'end').catch(() => undefined)
      }
    })()
  return (
    <section
      className={`call-panel ${connected ? 'call-panel-connected' : ''}`}
      aria-label="Private call"
      aria-live="polite"
    >
      <header className="workspace-header">
        <div className="min-w-0 flex-1">
          <strong className="block truncate">{members[peerId]?.display_name ?? 'Private call'}</strong>
          <p className="text-xs text-[var(--text-muted)]">
            {connected
              ? 'Private call'
              : call.status === 'accepted'
                ? 'Call ready'
                : incoming
                  ? 'Incoming call'
                  : 'Calling…'}
          </p>
        </div>
        <Phone size={18} />
      </header>
      {connected && <VoiceStage />}
      <div className="flex flex-wrap justify-center gap-2 p-4">
        {connected ? (
          <>
            <div className="w-16"><VoiceDeviceControl kind="audioinput" /></div>
            <div className="w-16"><VoiceDeviceControl kind="audiooutput" /></div>
            <button
              className="btn btn-subtle"
              aria-label="Toggle camera"
              aria-pressed={voice.cameraOn}
              onClick={() => void voice.toggleCamera()}
            >
              <Video size={18} />
            </button>
            <button className="btn btn-subtle" aria-label="Share screen" onClick={onShare}>
              <ScreenShare size={18} />
            </button>
          </>
        ) : (
          (incoming || call.status === 'accepted') && (
            <button className="btn btn-primary" disabled={voice.connecting} onClick={join}>
              {call.status === 'ringing' ? 'Accept' : 'Join call'}
            </button>
          )
        )}
        <button
          className="btn btn-danger"
          onClick={() =>
            void direct
              .action(call.id, 'end')
              .then(() => {
                setCalls((old) => old.filter((c) => c.id !== call.id))
                if (voice.channelId === `direct:${call.id}`) void voice.leave()
              })
              .catch((e) => toast.error(e.message))
          }
        >
          <PhoneOff size={18} />
          {connected ? 'End' : incoming ? 'Decline' : 'Cancel'}
        </button>
      </div>
      {voice.channelId === `direct:${call.id}` && voice.error && (
        <p role="alert" className="px-4 pb-4 text-sm text-[var(--danger)]">
          {voice.error}
        </p>
      )}
    </section>
  )
}
