import { dimensionsFor, type ScreenShareSettings } from './screenshare'

declare global {
  interface Window {
    __MINICHAT_NATIVE_CAPTURE__?: boolean
  }
}
export const hasNativeCapture = () => Boolean(window.__MINICHAT_NATIVE_CAPTURE__)
export const cancelNativePicker = () => {
  if (hasNativeCapture()) void invoke('stop_capture').catch(() => undefined)
}

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const bridge = window.__TAURI__?.core
  if (!bridge)
    return Promise.reject(new Error('Update the MiniChat desktop app to use its screen picker.'))
  return bridge.invoke<T>(command, args)
}

/** Backpressure: only one native frame and one decode may be in flight. */
export async function nativeScreenTrack(settings: ScreenShareSettings): Promise<{
  track: MediaStreamTrack
  audio?: MediaStreamTrack
  stop: () => void
}> {
  // Unlock audio in the original Share click, before waiting for the native
  // window. The click in another webview cannot grant this page activation.
  let audioContext: AudioContext | undefined
  let resumed = Promise.resolve(false)
  try {
    audioContext = new AudioContext({ sampleRate: 48000 })
    resumed = audioContext.resume().then(
      () => true,
      () => false,
    )
  } catch {
    // Video-only sharing remains available without an audio device.
  }
  let selected: { session: number; name: string; audio: boolean }
  try {
    selected = await invoke('choose_capture')
  } catch (error) {
    void audioContext?.close().catch(() => undefined)
    throw error
  }
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d', { alpha: false })!
  const height = dimensionsFor(settings.resolution)?.height ?? 2160
  let ended = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let track: MediaStreamTrack | undefined
  let audioTimer: ReturnType<typeof setTimeout> | undefined
  let audio: MediaStreamTrack | undefined
  const stop = () => {
    if (ended) return
    ended = true
    clearTimeout(timer)
    clearTimeout(audioTimer)
    track?.stop()
    audio?.stop()
    void audioContext?.close().catch(() => undefined)
    void invoke('stop_capture', { session: selected.session }).catch(() => undefined)
  }
  const draw = async () => {
    const bytes = await invoke<ArrayBuffer>('capture_frame', {
      session: selected.session,
      height,
    })
    const image = await createImageBitmap(new Blob([bytes], { type: 'image/jpeg' }))
    try {
      if (ended) return
      if (canvas.width !== image.width || canvas.height !== image.height) {
        canvas.width = image.width
        canvas.height = image.height
      }
      context.drawImage(image, 0, 0)
    } finally {
      image.close()
    }
  }
  try {
    await draw()
    track = canvas.captureStream(settings.fps).getVideoTracks()[0]
    track.contentHint = settings.content
    if (selected.audio) {
      if (!audioContext || !(await resumed)) throw new Error('Could not start shared audio.')
      const destination = audioContext.createMediaStreamDestination()
      audio = destination.stream.getAudioTracks()[0]
      let scheduled = 0
      const pumpAudio = async () => {
        try {
          const bytes = await invoke<ArrayBuffer>('capture_audio', {
            session: selected.session,
          })
          if (ended || !audioContext) return
          const samples = new Float32Array(bytes)
          const frames = Math.floor(samples.length / 2)
          if (frames) {
            const buffer = audioContext.createBuffer(2, frames, 48000)
            for (let channel = 0; channel < 2; channel++) {
              const data = buffer.getChannelData(channel)
              for (let i = 0; i < frames; i++) data[i] = samples[i * 2 + channel]
            }
            // Bound scheduled latency after a background pause or slow IPC.
            if (scheduled < audioContext.currentTime || scheduled > audioContext.currentTime + 0.5)
              scheduled = audioContext.currentTime + 0.04
            const source = audioContext.createBufferSource()
            source.buffer = buffer
            source.connect(destination)
            source.start(scheduled)
            source.onended = () => source.disconnect()
            scheduled += buffer.duration
          }
          audioTimer = setTimeout(() => void pumpAudio(), 20)
        } catch {
          stop()
          track?.dispatchEvent(new Event('ended'))
        }
      }
      void pumpAudio()
    } else {
      void audioContext?.close().catch(() => undefined)
      audioContext = undefined
    }
    const next = async () => {
      const started = performance.now()
      try {
        await draw()
      } catch {
        stop()
        track?.dispatchEvent(new Event('ended'))
        return
      }
      if (!ended)
        timer = setTimeout(
          () => void next(),
          Math.max(0, 1000 / settings.fps - (performance.now() - started)),
        )
    }
    timer = setTimeout(() => void next(), 1000 / settings.fps)
    track.addEventListener('ended', stop, { once: true })
    return { track, audio, stop }
  } catch (error) {
    stop()
    throw error
  }
}
