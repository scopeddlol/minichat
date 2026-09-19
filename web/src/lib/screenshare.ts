/**
 * Screen-share quality.
 *
 * What we control: the capture resolution and frame rate we ask for, the
 * encoder bitrate we publish at, and the content hint that tells the encoder
 * whether to protect motion or detail.
 *
 * Browsers keep their own source picker and sharing indicator. The Windows
 * desktop client instead uses the bundled native picker and frame/audio
 * bridge in nativeCapture.ts, with an explicit selection and stop control.
 */

export type ScreenResolution = '480' | '720' | '1080' | '1440' | 'native'
export type ScreenFps = 15 | 30 | 60
/** What to protect when bandwidth runs short. */
export type ScreenContent = 'motion' | 'detail'

export interface ScreenShareSettings {
  resolution: ScreenResolution
  fps: ScreenFps
  content: ScreenContent
  /** Which tab of the browser's picker to open on. */
  source: 'monitor' | 'window'
  /** Try to include system audio in the share. */
  audio: boolean
}

export const DEFAULT_SCREENSHARE: ScreenShareSettings = {
  resolution: '1080',
  fps: 30,
  content: 'motion',
  source: 'monitor',
  audio: true,
}

const STORAGE_KEY = 'minichat.screenshare'

export function loadScreenShareSettings(): ScreenShareSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_SCREENSHARE }
    return { ...DEFAULT_SCREENSHARE, ...(JSON.parse(raw) as Partial<ScreenShareSettings>) }
  } catch {
    return { ...DEFAULT_SCREENSHARE }
  }
}

export function saveScreenShareSettings(settings: ScreenShareSettings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  } catch {
    /* private browsing — the choice just won't persist */
  }
}

export const RESOLUTIONS: { value: ScreenResolution; label: string; hint: string }[] = [
  { value: '480', label: '480p', hint: 'Lowest bandwidth' },
  { value: '720', label: '720p', hint: 'Good on slow connections' },
  { value: '1080', label: '1080p', hint: 'Recommended' },
  { value: '1440', label: '1440p', hint: 'Sharp, needs headroom' },
  { value: 'native', label: 'Native', hint: 'Your display, uncapped' },
]

export const FRAME_RATES: { value: ScreenFps; label: string; hint: string }[] = [
  { value: 15, label: '15 fps', hint: 'Slides and documents' },
  { value: 30, label: '30 fps', hint: 'Recommended' },
  { value: 60, label: '60 fps', hint: 'Games and video' },
]

/** Pixel dimensions for a resolution, or null to leave capture uncapped. */
export function dimensionsFor(resolution: ScreenResolution): { width: number; height: number } | null {
  switch (resolution) {
    case '480':
      return { width: 854, height: 480 }
    case '720':
      return { width: 1280, height: 720 }
    case '1080':
      return { width: 1920, height: 1080 }
    case '1440':
      return { width: 2560, height: 1440 }
    case 'native':
      return null
  }
}

/**
 * Bitrate ceiling in bits per second.
 *
 * Scaled by both pixel count and frame rate: 1440p60 needs roughly eight times
 * the budget of 720p30, and guessing one number for all of them means either
 * a smeared high-resolution share or a wastefully fat low-resolution one.
 */
export function bitrateFor(resolution: ScreenResolution, fps: ScreenFps): number {
  const base: Record<ScreenResolution, number> = {
    '480': 800_000,
    '720': 1_800_000,
    '1080': 3_500_000,
    '1440': 6_000_000,
    native: 8_000_000,
  }
  const fpsScale = fps === 60 ? 1.6 : fps === 15 ? 0.7 : 1
  return Math.round(base[resolution] * fpsScale)
}

/** Capture options for `setScreenShareEnabled`. */
export function captureOptions(settings: ScreenShareSettings) {
  const dimensions = dimensionsFor(settings.resolution)
  return {
    audio: settings.audio,
    // Opens the browser picker on the tab the member actually wants.
    video: { displaySurface: settings.source } as const,
    ...(dimensions
      ? { resolution: { ...dimensions, frameRate: settings.fps } }
      : // Native resolution: still pin the frame rate, just not the size.
        { resolution: { width: 3840, height: 2160, frameRate: settings.fps } }),
    // 'motion' protects frame rate, 'detail' protects sharpness.
    contentHint: settings.content === 'motion' ? ('motion' as const) : ('detail' as const),
    // Never offer the MiniChat window itself — sharing it is an infinite
    // mirror and never what anyone meant to do.
    selfBrowserSurface: 'exclude' as const,
    // Let the member switch source mid-share instead of stopping and
    // restarting, where the browser supports it.
    surfaceSwitching: 'include' as const,
    systemAudio: settings.audio ? ('include' as const) : ('exclude' as const),
  }
}

/** Publish options for `setScreenShareEnabled`. */
export function publishOptions(settings: ScreenShareSettings) {
  return {
    screenShareEncoding: {
      maxBitrate: bitrateFor(settings.resolution, settings.fps),
      maxFramerate: settings.fps,
    },
    // Keep the picture sharp and drop frames instead when bandwidth tightens,
    // unless the member said they care about motion.
    degradationPreference:
      settings.content === 'motion'
        ? ('maintain-framerate' as RTCDegradationPreference)
        : ('maintain-resolution' as RTCDegradationPreference),
  }
}
