import {
  ConnectionState,
  LocalTrackPublication,
  RemoteTrack,
  RemoteTrackPublication,
  Room,
  RoomEvent,
  Track,
  type RemoteParticipant,
} from 'livekit-client'
import { create } from 'zustand'
import { api } from './api'
import { gateway } from './gateway'
import { useStore } from './store'

export interface VoiceParticipant {
  identity: string
  name: string
  speaking: boolean
  muted: boolean
  hasVideo: boolean
  hasScreenShare: boolean
  isLocal: boolean
  connectionQuality: string
}

interface VoiceStore {
  room: Room | null
  channelId: string | null
  connecting: boolean
  connected: boolean
  error: string

  muted: boolean
  deafened: boolean
  cameraOn: boolean
  screenSharing: boolean

  canSpeak: boolean
  canVideo: boolean
  canScreenShare: boolean

  participants: VoiceParticipant[]
  /** Tracks keyed by `${identity}:${source}` so tiles can attach them. */
  tracks: Record<string, Track>
  focusedIdentity: string | null

  join: (channelId: string) => Promise<void>
  leave: () => Promise<void>
  toggleMute: () => Promise<void>
  toggleDeafen: () => Promise<void>
  toggleCamera: () => Promise<void>
  toggleScreenShare: () => Promise<void>
  setFocus: (identity: string | null) => void
}

const trackKey = (identity: string, source: Track.Source) => `${identity}:${source}`

export const useVoice = create<VoiceStore>((set, get) => ({
  room: null,
  channelId: null,
  connecting: false,
  connected: false,
  error: '',

  muted: false,
  deafened: false,
  cameraOn: false,
  screenSharing: false,

  canSpeak: true,
  canVideo: true,
  canScreenShare: true,

  participants: [],
  tracks: {},
  focusedIdentity: null,

  async join(channelId) {
    if (get().connecting) return
    if (get().channelId === channelId && get().connected) return
    if (get().room) await get().leave()

    set({ connecting: true, error: '', channelId })

    try {
      const grant = await api.voiceToken(channelId)
      const room = new Room({
        adaptiveStream: true,
        dynacast: true,
        // Browser defaults are tuned for conferencing; this is closer to what
        // a casual voice channel wants.
        audioCaptureDefaults: {
          autoGainControl: true,
          echoCancellation: true,
          noiseSuppression: true,
        },
        publishDefaults: {
          dtx: true,
          red: true,
        },
      })

      wireEvents(room, set, get)
      await room.connect(grant.url, grant.token)

      set({
        room,
        connected: true,
        connecting: false,
        canSpeak: grant.can_speak,
        canVideo: grant.can_video,
        canScreenShare: grant.can_screen_share,
        muted: !grant.can_speak,
        cameraOn: false,
        screenSharing: false,
      })

      if (grant.can_speak) {
        try {
          await room.localParticipant.setMicrophoneEnabled(true)
        } catch {
          // No microphone, or permission denied: stay connected as a listener.
          set({ muted: true, error: 'Microphone unavailable — you joined as a listener.' })
        }
      }

      syncParticipants(room, set)
      publishVoiceState(channelId, get())
    } catch (error) {
      set({
        connecting: false,
        connected: false,
        channelId: null,
        error: error instanceof Error ? error.message : 'Could not join voice.',
      })
      throw error
    }
  },

  async leave() {
    const room = get().room
    set({
      room: null,
      channelId: null,
      connected: false,
      connecting: false,
      participants: [],
      tracks: {},
      cameraOn: false,
      screenSharing: false,
      deafened: false,
      focusedIdentity: null,
    })
    if (room) {
      try {
        await room.disconnect()
      } catch {
        /* already gone */
      }
    }
    gateway.send({ op: 'voice_state', channel_id: null })
  },

  async toggleMute() {
    const { room, muted, canSpeak, channelId } = get()
    if (!room || !canSpeak) return
    const next = !muted
    try {
      await room.localParticipant.setMicrophoneEnabled(!next)
      set({ muted: next })
      if (channelId) publishVoiceState(channelId, { ...get(), muted: next })
    } catch {
      set({ error: 'Could not change your microphone.' })
    }
  },

  async toggleDeafen() {
    const { room, deafened, muted, channelId } = get()
    if (!room) return
    const next = !deafened
    // Deafening implies muting, the same way it does in Discord.
    room.remoteParticipants.forEach((participant) => participant.setVolume(next ? 0 : 1))
    const nextMuted = next ? true : muted
    if (next !== deafened) {
      try {
        await room.localParticipant.setMicrophoneEnabled(!nextMuted)
      } catch {
        /* keep going: deafen still applies locally */
      }
    }
    set({ deafened: next, muted: nextMuted })
    if (channelId) publishVoiceState(channelId, { ...get(), deafened: next, muted: nextMuted })
  },

  async toggleCamera() {
    const { room, cameraOn, canVideo, channelId } = get()
    if (!room || !canVideo) return
    const next = !cameraOn
    try {
      await room.localParticipant.setCameraEnabled(next)
      set({ cameraOn: next })
      if (channelId) publishVoiceState(channelId, { ...get(), cameraOn: next })
    } catch {
      set({ error: 'Could not access your camera.' })
    }
  },

  async toggleScreenShare() {
    const { room, screenSharing, canScreenShare, channelId } = get()
    if (!room || !canScreenShare) return
    const next = !screenSharing
    try {
      await room.localParticipant.setScreenShareEnabled(next, { audio: true })
      set({ screenSharing: next })
      if (channelId) publishVoiceState(channelId, { ...get(), screenSharing: next })
    } catch {
      // The user dismissing the picker is not an error worth surfacing.
      set({ screenSharing: false })
    }
  },

  setFocus(identity) {
    set({ focusedIdentity: identity })
  },
}))

type Setter = (partial: Partial<VoiceStore>) => void

function wireEvents(room: Room, set: Setter, get: () => VoiceStore) {
  const resync = () => syncParticipants(room, set)

  room
    .on(RoomEvent.ParticipantConnected, resync)
    .on(RoomEvent.ParticipantDisconnected, resync)
    .on(RoomEvent.ActiveSpeakersChanged, resync)
    .on(RoomEvent.TrackMuted, resync)
    .on(RoomEvent.TrackUnmuted, resync)
    .on(RoomEvent.ConnectionQualityChanged, resync)
    .on(RoomEvent.LocalTrackPublished, (publication: LocalTrackPublication) => {
      if (publication.track) {
        set({
          tracks: {
            ...get().tracks,
            [trackKey(room.localParticipant.identity, publication.source)]: publication.track,
          },
        })
      }
      resync()
    })
    .on(RoomEvent.LocalTrackUnpublished, (publication: LocalTrackPublication) => {
      const tracks = { ...get().tracks }
      delete tracks[trackKey(room.localParticipant.identity, publication.source)]
      set({ tracks })
      // The browser's own "stop sharing" button ends the track without going
      // through our toggle, so mirror the state back here.
      if (publication.source === Track.Source.ScreenShare) set({ screenSharing: false })
      if (publication.source === Track.Source.Camera) set({ cameraOn: false })
      resync()
    })
    .on(
      RoomEvent.TrackSubscribed,
      (track: RemoteTrack, publication: RemoteTrackPublication, participant: RemoteParticipant) => {
        if (get().deafened && track.kind === Track.Kind.Audio) participant.setVolume(0)
        set({
          tracks: { ...get().tracks, [trackKey(participant.identity, publication.source)]: track },
        })
        resync()
      },
    )
    .on(
      RoomEvent.TrackUnsubscribed,
      (_track: RemoteTrack, publication: RemoteTrackPublication, participant: RemoteParticipant) => {
        const tracks = { ...get().tracks }
        delete tracks[trackKey(participant.identity, publication.source)]
        set({ tracks })
        resync()
      },
    )
    .on(RoomEvent.Disconnected, () => {
      set({ connected: false, room: null, channelId: null, participants: [], tracks: {} })
      gateway.send({ op: 'voice_state', channel_id: null })
    })
    .on(RoomEvent.ConnectionStateChanged, (state: ConnectionState) => {
      set({ connected: state === ConnectionState.Connected })
    })
}

function syncParticipants(room: Room, set: Setter) {
  const build = (participant: RemoteParticipant | typeof room.localParticipant, isLocal: boolean): VoiceParticipant => ({
    identity: participant.identity,
    name: participant.name || participant.identity,
    speaking: participant.isSpeaking,
    muted: !participant.isMicrophoneEnabled,
    hasVideo: participant.isCameraEnabled,
    hasScreenShare: participant.isScreenShareEnabled,
    isLocal,
    connectionQuality: String(participant.connectionQuality),
  })

  const participants = [
    build(room.localParticipant, true),
    ...Array.from(room.remoteParticipants.values()).map((p) => build(p, false)),
  ]
  set({ participants })
}

function publishVoiceState(channelId: string, state: Partial<VoiceStore>) {
  gateway.send({
    op: 'voice_state',
    channel_id: channelId,
    muted: state.muted ?? false,
    deafened: state.deafened ?? false,
    video: state.cameraOn ?? false,
    streaming: state.screenSharing ?? false,
  })
}

/** Disconnect when a moderator drops us from voice. */
gateway.on((event) => {
  if (event.t === 'VOICE_FORCE_DISCONNECT') void useVoice.getState().leave()
  if (event.t === 'CHANNEL_DELETE' && useVoice.getState().channelId === event.d.id) {
    void useVoice.getState().leave()
  }
  if (event.t === 'INVALID_SESSION') void useVoice.getState().leave()
})

/** Keep the roster fresh when the member list changes names/avatars. */
export function voiceMemberName(identity: string): string {
  const member = useStore.getState().members[identity]
  return member?.display_name ?? 'Unknown'
}
