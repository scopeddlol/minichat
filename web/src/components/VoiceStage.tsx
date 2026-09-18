import { Maximize2, MicOff, Minimize2, ScreenShare, UserX, Video, Volume2, VolumeX, Wifi } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Track, type Track as TrackType } from 'livekit-client'
import { api } from '../lib/api'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import { useVoice, type VoiceParticipant } from '../lib/voice'
import { Avatar, EmptyState, toast } from './ui'

/** The video grid shown while connected to a voice channel. */
export default function VoiceStage() {
  const { participants, tracks, focusedIdentity, setFocus } = useVoice()
  const members = useStore((s) => s.members)
  const permissions = useStore((s) => s.permissions)
  const isDirect = useVoice(s => s.channelId?.startsWith('direct:'))

  // A screen share takes over the stage, since that's what people came to see.
  const sharer = participants.find((p) => p.hasScreenShare)
  const focused = focusedIdentity
    ? participants.find((p) => p.identity === focusedIdentity)
    : sharer

  if (!participants.length) {
    return (
      <div className="flex-1 flex items-center justify-center" style={{ background: 'var(--surface-0)' }}>
        <EmptyState icon={<Wifi size={22} />} title="Connecting to voice…" />
      </div>
    )
  }

  const tileCount = participants.length
  const columns = focused ? 1 : tileCount <= 1 ? 1 : tileCount <= 4 ? 2 : tileCount <= 9 ? 3 : 4

  return (
    <div className="flex-1 min-h-0 flex flex-col gap-2 p-3" style={{ background: 'var(--surface-0)' }}>
      {focused && (
        <div className="flex-1 min-h-0 relative">
          <Tile
            participant={focused}
            track={
              tracks[`${focused.identity}:${Track.Source.ScreenShare}`] ??
              tracks[`${focused.identity}:${Track.Source.Camera}`]
            }
            member={members[focused.identity]}
            frame={members[focused.identity]?.avatar_frame}
            large
            canModerate={!isDirect && can(permissions, P.MOVE_MEMBERS)}
          />
          <button
            className="absolute top-2.5 right-2.5 btn btn-subtle !p-1.5"
            onClick={() => setFocus(null)}
            title="Exit focus"
          >
            <Minimize2 size={15} />
          </button>
        </div>
      )}

      <div
        className={`grid gap-2 ${focused ? 'shrink-0' : 'flex-1 min-h-0'}`}
        style={{
          gridTemplateColumns: `repeat(${focused ? Math.min(tileCount, 6) : columns}, minmax(0, 1fr))`,
          height: focused ? 116 : undefined,
        }}
      >
        {participants
          .filter((p) => !focused || p.identity !== focused.identity)
          .map((participant) => (
            <Tile
              key={participant.identity}
              participant={participant}
              track={tracks[`${participant.identity}:${Track.Source.Camera}`]}
              member={members[participant.identity]}
              frame={members[participant.identity]?.avatar_frame}
              compact={Boolean(focused)}
              onFocus={() => setFocus(participant.identity)}
              canModerate={!isDirect && can(permissions, P.MOVE_MEMBERS)}
            />
          ))}
      </div>
    </div>
  )
}

function Tile({
  participant,
  track,
  member,
  frame,
  large,
  compact,
  onFocus,
  canModerate,
}: {
  participant: VoiceParticipant
  track?: TrackType
  member?: { display_name: string; avatar_url: string | null; accent_color: string; id: string }
  /** Passed separately so the trimmed `member` shape above stays trimmed. */
  frame?: import('../lib/types').ImageFrame | null
  large?: boolean
  compact?: boolean
  onFocus?: () => void
  canModerate?: boolean
}) {
  const videoRef = useRef<HTMLVideoElement>(null)
  const [volumeOpen, setVolumeOpen] = useState(false)
  const volumes = useVoice((s) => s.volumes)
  const setUserVolume = useVoice((s) => s.setUserVolume)
  const volume = volumes[participant.identity] ?? 1
  const name = member?.display_name ?? participant.name

  useEffect(() => {
    const element = videoRef.current
    if (!element || !track) return
    track.attach(element)
    return () => {
      track.detach(element)
    }
  }, [track])

  const hasVisual = Boolean(track) && (participant.hasVideo || participant.hasScreenShare)

  return (
    <div
      className="relative rounded-xl overflow-hidden border flex items-center justify-center group transition-all"
      style={{
        background: 'var(--surface-1)',
        borderColor: participant.speaking && !participant.muted ? 'var(--accent)' : 'var(--border)',
        boxShadow: participant.speaking && !participant.muted ? '0 0 0 1px var(--accent), 0 0 24px -6px var(--accent-glow)' : undefined,
        minHeight: compact ? 0 : 120,
      }}
    >
      {hasVisual ? (
        <video
          ref={videoRef}
          autoPlay
          playsInline
          muted={participant.isLocal}
          className="w-full h-full object-contain"
          style={{ background: '#000' }}
        />
      ) : (
        <Avatar
          id={member?.id ?? participant.identity}
          name={name}
          src={member?.avatar_url}
          frame={frame}
          accent={member?.accent_color}
          size={large ? 'xxl' : compact ? 'md' : 'xl'}
          ring={participant.speaking && !participant.muted}
          className={participant.speaking && !participant.muted ? 'speaking-ring rounded-full' : ''}
        />
      )}

      <div className="absolute inset-x-0 bottom-0 px-2 py-1.5 flex items-center gap-1.5 bg-gradient-to-t from-black/70 to-transparent">
        <span className="text-[0.72rem] font-medium text-white truncate flex-1">
          {name}
          {participant.isLocal && ' (you)'}
        </span>
        {participant.hasScreenShare && <ScreenShare size={12} className="text-white shrink-0" />}
        {participant.hasVideo && <Video size={12} className="text-white shrink-0" />}
        {participant.muted && <MicOff size={12} className="shrink-0" style={{ color: 'var(--danger)' }} />}
      </div>

      {volumeOpen && !participant.isLocal && (
        <div
          className="absolute bottom-9 left-1.5 right-1.5 z-10 flex items-center gap-2 px-2 py-1.5 rounded-lg animate-pop-in"
          style={{ background: 'var(--surface-0)', boxShadow: 'var(--shadow-md)' }}
        >
          <button
            onClick={() => setUserVolume(participant.identity, volume === 0 ? 1 : 0)}
            style={{ color: volume === 0 ? 'var(--danger)' : 'var(--text-muted)' }}
            aria-label={volume === 0 ? `Unmute ${name}` : `Mute ${name}`}
          >
            {volume === 0 ? <VolumeX size={13} /> : <Volume2 size={13} />}
          </button>
          <input
            type="range"
            min={0}
            max={2}
            step={0.05}
            value={volume}
            onChange={(event) => setUserVolume(participant.identity, Number(event.target.value))}
            className="flex-1 accent-[var(--accent)]"
            aria-label={`Volume for ${name}`}
          />
          <span className="text-[0.62rem] tabular-nums w-8 text-right" style={{ color: 'var(--text-faint)' }}>
            {Math.round(volume * 100)}
          </span>
        </div>
      )}

      <div className="absolute top-1.5 right-1.5 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        {!participant.isLocal && (
          <button
            className="btn btn-subtle !p-1"
            onClick={() => setVolumeOpen((value) => !value)}
            title={`Volume for ${name}`}
            style={volume === 0 ? { color: 'var(--danger)' } : undefined}
          >
            {volume === 0 ? <VolumeX size={12} /> : <Volume2 size={12} />}
          </button>
        )}
        {onFocus && hasVisual && (
          <button className="btn btn-subtle !p-1" onClick={onFocus} title="Focus">
            <Maximize2 size={12} />
          </button>
        )}
        {canModerate && !participant.isLocal && (
          <button
            className="btn btn-danger !p-1"
            title="Disconnect from voice"
            onClick={async () => {
              try {
                await api.forceDisconnect(participant.identity)
                toast.success(`${name} was disconnected.`)
              } catch (error) {
                toast.error(error instanceof Error ? error.message : 'Could not disconnect them.')
              }
            }}
          >
            <UserX size={12} />
          </button>
        )}
      </div>
    </div>
  )
}
