import { useEffect, useRef, useState } from 'react'
import { RoomEvent, Track, type RemoteAudioTrack } from 'livekit-client'
import { useVoice } from '../lib/voice'
import { unlockVoiceSounds } from '../lib/voiceSounds'

/** Own playback outside the stage so navigating to text or DMs cannot mute a call. */
export default function VoiceAudio() {
  const room = useVoice(s => s.room)
  const tracks = useVoice(s => s.tracks)
  const [blocked, setBlocked] = useState(false)
  useEffect(() => {
    if (!room) { setBlocked(false); return }
    const update = () => setBlocked(!room.canPlaybackAudio)
    room.on(RoomEvent.AudioPlaybackStatusChanged, update)
    update()
    return () => { room.off(RoomEvent.AudioPlaybackStatusChanged, update) }
  }, [room])
  return <>
    {Object.entries(tracks).filter(([, track]) => track.kind === Track.Kind.Audio && !track.isLocal).map(([key, track]) =>
      <AudioTrack key={key} track={track as RemoteAudioTrack} />,
    )}
    {room && blocked && <button className="fixed bottom-24 left-1/2 -translate-x-1/2 z-[80] btn btn-primary" onClick={() => {
      unlockVoiceSounds()
      void room.startAudio().then(() => setBlocked(!room.canPlaybackAudio)).catch(() => setBlocked(true))
    }}>Enable call audio</button>}
  </>
}

function AudioTrack({ track }: { track: RemoteAudioTrack }) {
  const ref = useRef<HTMLAudioElement>(null)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    track.attach(element)
    return () => { track.detach(element) }
  }, [track])
  return <audio ref={ref} autoPlay />
}
