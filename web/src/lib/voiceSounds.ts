type Cue = 'join' | 'leave' | 'mute' | 'unmute' | 'deafen' | 'undeafen'
let context: AudioContext | undefined

/** Short original tones, played locally rather than injected into a microphone. */
export function unlockVoiceSounds() {
  try {
    context ??= new AudioContext()
    void context.resume().catch(() => undefined)
  } catch {
    /* Audio is unavailable on this device. */
  }
}

export function playVoiceSound(cue: Cue, output = 'default') {
  unlockVoiceSounds()
  const ctx = context
  if (!ctx) return
  const sink = ctx as AudioContext & {
    setSinkId?: (id: string) => Promise<void>
  }
  void (async () => {
    try {
      // A saved headset may have been unplugged. Still play on the default
      // output instead of swallowing the entire cue when routing fails.
      if (sink.setSinkId) {
        try { await sink.setSinkId(output === 'default' ? '' : output) }
        catch { await sink.setSinkId('').catch(() => undefined) }
      }
      await ctx.resume()
      const notes: Record<Cue, number[]> = {
        join: [440, 660],
        leave: [660, 440],
        mute: [330, 260],
        unmute: [260, 390],
        deafen: [330, 220, 165],
        undeafen: [165, 220, 330],
      }
      notes[cue].forEach((frequency, index) => {
        const at = ctx.currentTime + index * 0.085
        const oscillator = ctx.createOscillator()
        const gain = ctx.createGain()
        oscillator.type = 'sine'
        oscillator.frequency.value = frequency
        gain.gain.setValueAtTime(0, at)
        gain.gain.linearRampToValueAtTime(0.09, at + 0.008)
        gain.gain.exponentialRampToValueAtTime(0.001, at + 0.13)
        oscillator.connect(gain).connect(ctx.destination)
        oscillator.start(at)
        oscillator.stop(at + 0.14)
        oscillator.onended = () => {
          oscillator.disconnect()
          gain.disconnect()
        }
      })
    } catch {
      /* A removed output device must not break voice controls. */
    }
  })()
}
