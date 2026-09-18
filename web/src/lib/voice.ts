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
import { direct } from './direct'
import {
  captureOptions, loadScreenShareSettings, publishOptions, saveScreenShareSettings,
  type ScreenShareSettings,
} from './screenshare'
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

export interface MediaDeviceOption {
  deviceId: string
  label: string
}

export interface DeviceSelection {
  audioinput: string
  videoinput: string
  audiooutput: string
}

const DEVICE_STORAGE_KEY = 'minichat.devices'
const VOLUME_STORAGE_KEY = 'minichat.volumes'

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? { ...fallback, ...(JSON.parse(raw) as T) } : fallback
  } catch {
    return fallback
  }
}

function saveJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* private browsing — selection just won't persist */
  }
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

  /** Available hardware, refreshed after permission is granted. */
  devices: Record<keyof DeviceSelection, MediaDeviceOption[]>
  selectedDevices: DeviceSelection
  /** Per-member playback volume, 0-2, persisted locally. */
  volumes: Record<string, number>
  /** True while push-to-talk is holding the microphone open. */
  pushToTalkActive: boolean
  /** Capture quality for the next screen share. */
  screenShare: ScreenShareSettings

  join: (channelId: string) => Promise<void>
  leave: () => Promise<void>
  toggleMute: () => Promise<void>
  toggleDeafen: () => Promise<void>
  toggleCamera: () => Promise<void>
  toggleScreenShare: () => Promise<void>
  setFocus: (identity: string | null) => void
  refreshDevices: () => Promise<void>
  selectDevice: (kind: keyof DeviceSelection, deviceId: string) => Promise<void>
  setUserVolume: (userId: string, volume: number) => void
  setPushToTalk: (active: boolean) => void
  setScreenShareSettings: (settings: Partial<ScreenShareSettings>) => void
  startScreenShare: () => Promise<void>
  stopScreenShare: () => Promise<void>
}

const trackKey = (identity: string, source: Track.Source) => `${identity}:${source}`
let joinAttempt = 0
let pendingRoom: Room | null = null

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

  devices: { audioinput: [], videoinput: [], audiooutput: [] },
  selectedDevices: loadJson<DeviceSelection>(DEVICE_STORAGE_KEY, {
    audioinput: 'default',
    videoinput: 'default',
    audiooutput: 'default',
  }),
  volumes: loadJson<Record<string, number>>(VOLUME_STORAGE_KEY, {}),
  pushToTalkActive: false,
  screenShare: loadScreenShareSettings(),

  async join(channelId) {
    if (get().connecting) return
    if (get().channelId === channelId && get().connected) return
    if (get().room) await get().leave()
    const attempt = ++joinAttempt

    set({ connecting: true, error: '', channelId })

    try {
      const grant = await api.voiceToken(channelId)
      if (attempt !== joinAttempt) return
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

      pendingRoom = room
      wireEvents(room, set, get)
      await room.connect(grant.url, grant.token)
      if (attempt !== joinAttempt) { await room.disconnect(); return }
      pendingRoom = null

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

      // Honour a saved microphone choice rather than always taking the default.
      const chosen = get().selectedDevices
      if (chosen.audioinput && chosen.audioinput !== 'default') {
        await room
          .switchActiveDevice('audioinput', chosen.audioinput)
          .catch(() => undefined)
      }
      if (chosen.audiooutput && chosen.audiooutput !== 'default') {
        await room
          .switchActiveDevice('audiooutput', chosen.audiooutput)
          .catch(() => undefined)
      }

      // Push-to-talk starts closed: the mic opens only while the key is held.
      const hotkeys = loadHotkeyPreferences()
      const startMuted = hotkeys.enabled && hotkeys.pttMode === 'hold'

      if (grant.can_speak) {
        try {
          await room.localParticipant.setMicrophoneEnabled(!startMuted)
          if (startMuted) set({ muted: true })
        } catch {
          // No microphone, or permission denied: stay connected as a listener.
          set({ muted: true, error: 'Microphone unavailable — you joined as a listener.' })
        }
      }

      void get().refreshDevices()

      syncParticipants(room, set)
      publishVoiceState(channelId, get())
    } catch (error) {
      if (attempt !== joinAttempt) return
      await pendingRoom?.disconnect().catch(() => undefined)
      pendingRoom = null
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
    ++joinAttempt
    const pending = pendingRoom
    pendingRoom = null
    const room = get().room
    const leavingChannel = get().channelId
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
    if (pending && pending !== room) await pending.disconnect().catch(() => undefined)
    gateway.send({ op: 'voice_state', channel_id: null })
    if (leavingChannel?.startsWith('direct:')) await direct.action(leavingChannel.slice(7), 'end').catch(() => undefined)
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
    // Deafening implies muting, the same way it does in Discord. Undeafening
    // restores each member's own volume rather than resetting everyone to 1.
    const volumes = get().volumes
    room.remoteParticipants.forEach((participant) =>
      participant.setVolume(next ? 0 : (volumes[participant.identity] ?? 1)),
    )
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
    if (get().screenSharing) await get().stopScreenShare()
    else await get().startScreenShare()
  },

  setScreenShareSettings(patch) {
    const settings = { ...get().screenShare, ...patch }
    set({ screenShare: settings })
    saveScreenShareSettings(settings)
  },

  /**
   * Begin sharing at the chosen quality.
   *
   * The browser's source picker appears after this call — it is the gate that
   * makes screen capture safe, and a page cannot replace it.
   */
  async startScreenShare() {
    const { room, canScreenShare, channelId, screenShare } = get()
    if (!room || !canScreenShare) return
    try {
      await room.localParticipant.setScreenShareEnabled(
        true,
        captureOptions(screenShare),
        publishOptions(screenShare),
      )
      set({ screenSharing: true })
      if (channelId) publishVoiceState(channelId, { ...get(), screenSharing: true })
    } catch (error) {
      // Dismissing the picker throws NotAllowedError; that's a choice, not a
      // fault, so it shouldn't raise an error banner.
      const name = (error as { name?: string })?.name
      set({
        screenSharing: false,
        error: name === 'NotAllowedError' ? '' : 'Could not start sharing your screen.',
      })
    }
  },

  async stopScreenShare() {
    const { room, channelId } = get()
    set({ screenSharing: false })
    if (!room) return
    try {
      await room.localParticipant.setScreenShareEnabled(false)
    } catch {
      /* already stopped */
    }
    if (channelId) publishVoiceState(channelId, { ...get(), screenSharing: false })
  },

  setFocus(identity) {
    set({ focusedIdentity: identity })
  },

  /**
   * Enumerate hardware. Labels are empty until the browser has granted media
   * permission at least once, which is why this runs again after joining.
   */
  async refreshDevices() {
    if (!navigator.mediaDevices?.enumerateDevices) return
    try {
      const all = await navigator.mediaDevices.enumerateDevices()
      const group = (kind: MediaDeviceKind): MediaDeviceOption[] =>
        all
          .filter((device) => device.kind === kind && device.deviceId)
          .map((device, index) => ({
            deviceId: device.deviceId,
            label: device.label || `${kind.replace('input', ' input').replace('output', ' output')} ${index + 1}`,
          }))

      set({
        devices: {
          audioinput: group('audioinput'),
          videoinput: group('videoinput'),
          audiooutput: group('audiooutput'),
        },
      })
    } catch {
      /* permission not granted yet; labels stay empty */
    }
  },

  async selectDevice(kind, deviceId) {
    const selected = { ...get().selectedDevices, [kind]: deviceId }
    set({ selectedDevices: selected })
    saveJson(DEVICE_STORAGE_KEY, selected)

    const room = get().room
    if (!room) return
    try {
      await room.switchActiveDevice(kind, deviceId)
    } catch {
      set({ error: 'Could not switch to that device.' })
    }
  },

  setUserVolume(userId, volume) {
    const clamped = Math.max(0, Math.min(2, volume))
    const volumes = { ...get().volumes, [userId]: clamped }
    set({ volumes })
    saveJson(VOLUME_STORAGE_KEY, volumes)

    const room = get().room
    if (!room) return
    const participant = room.remoteParticipants.get(userId)
    // Deafened means silent regardless of individual volumes.
    if (participant) participant.setVolume(get().deafened ? 0 : clamped)
  },

  /**
   * Open or close the microphone for push-to-talk. Kept separate from
   * `toggleMute` so the manual mute state isn't clobbered by a key press.
   */
  setPushToTalk(active) {
    const { room, canSpeak, channelId, deafened } = get()
    if (!room || !canSpeak || deafened) return
    set({ pushToTalkActive: active, muted: !active })
    room.localParticipant.setMicrophoneEnabled(active).catch(() => undefined)
    if (channelId) publishVoiceState(channelId, { ...get(), muted: !active })
  },
}))

/** Read hotkey preferences without importing the store and creating a cycle. */
function loadHotkeyPreferences(): { enabled: boolean; pttMode: 'hold' | 'toggle' } {
  try {
    const raw = localStorage.getItem('minichat.hotkeys')
    if (!raw) return { enabled: false, pttMode: 'hold' }
    const parsed = JSON.parse(raw) as { enabled?: boolean; pttMode?: 'hold' | 'toggle' }
    return { enabled: Boolean(parsed.enabled), pttMode: parsed.pttMode ?? 'hold' }
  } catch {
    return { enabled: false, pttMode: 'hold' }
  }
}

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
        if (track.kind === Track.Kind.Audio) {
          const stored = get().volumes[participant.identity]
          participant.setVolume(get().deafened ? 0 : (stored ?? 1))
        }
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
      if (get().room !== room) return
      const disconnectedChannel = get().channelId
      set({ connected: false, room: null, channelId: null, participants: [], tracks: {} })
      gateway.send({ op: 'voice_state', channel_id: null })
      if (disconnectedChannel?.startsWith('direct:')) void direct.action(disconnectedChannel.slice(7), 'end').catch(() => undefined)
    })
    .on(RoomEvent.ConnectionStateChanged, (state: ConnectionState) => {
      if (get().room !== room) return
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
  if (channelId.startsWith('direct:')) return
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
  if (event.t === 'ACCESS_UPDATE') {
    const channel = useVoice.getState().channelId
    if (channel && !channel.startsWith('direct:') && !event.d.channels.some((c: { id: string }) => c.id === channel)) void useVoice.getState().leave()
  }
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
