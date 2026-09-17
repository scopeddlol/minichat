import type { Instance, InstanceMeta } from './types'

/**
 * Everything that turns instance settings into what the app looks like.
 *
 * Applied from `/api/meta` before sign-in, so the login and invite pages are
 * already branded, and again from the gateway's READY payload once the full
 * instance record is available.
 */

export type ThemeChoice = 'dark' | 'light' | 'system'

const THEME_KEY = 'minichat.theme'
const CUSTOM_CSS_ID = 'minichat-instance-css'

/** What the instance wants when the member hasn't chosen for themselves. */
let instanceDefault: ThemeChoice = 'dark'

/** The member's own choice, or null when they've never set one. */
export function storedTheme(): ThemeChoice | null {
  try {
    const raw = localStorage.getItem(THEME_KEY)
    return raw === 'dark' || raw === 'light' || raw === 'system' ? raw : null
  } catch {
    return null
  }
}

export function effectiveTheme(): ThemeChoice {
  return storedTheme() ?? instanceDefault
}

export function setTheme(choice: ThemeChoice) {
  try {
    localStorage.setItem(THEME_KEY, choice)
  } catch {
    /* private browsing — the choice just won't persist */
  }
  applyTheme(choice)
}

/** Clear the member's override and fall back to the instance default. */
export function clearThemeOverride() {
  try {
    localStorage.removeItem(THEME_KEY)
  } catch {
    /* ignore */
  }
  applyTheme(instanceDefault)
}

export function applyTheme(choice: ThemeChoice = effectiveTheme()) {
  const prefersLight = window.matchMedia('(prefers-color-scheme: light)').matches
  const light = choice === 'light' || (choice === 'system' && prefersLight)

  const root = document.documentElement
  root.classList.toggle('light', light)
  root.classList.toggle('dark', !light)

  // Read the resolved background back out so the address bar and the PWA
  // splash match whatever tint the instance applied.
  const background = getComputedStyle(root).getPropertyValue('--bg').trim()
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute('content', background || (light ? '#f3f4f8' : '#0d0f16'))
}

/** Retint the app from a hex accent, deriving a readable foreground. */
export function applyAccent(hex: string) {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return
  const root = document.documentElement
  root.style.setProperty('--accent', hex)

  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
  root.style.setProperty('--accent-ink', luminance > 0.62 ? '#10131c' : '#ffffff')
}

type Branding = Pick<
  Instance,
  'accent_color' | 'surface_tint' | 'corner_radius' | 'font_family' | 'custom_css' | 'theme_mode'
>

/** Apply an instance's full visual identity. */
export function applyBranding(branding: Partial<Branding>) {
  const root = document.documentElement

  if (branding.accent_color) applyAccent(branding.accent_color)

  // Unsetting the property is what makes each surface fall back to its own
  // base colour, so clearing a tint genuinely restores the default palette.
  if (branding.surface_tint && /^#[0-9a-f]{6}$/i.test(branding.surface_tint)) {
    root.style.setProperty('--tint', branding.surface_tint)
  } else {
    root.style.removeProperty('--tint')
  }

  if (typeof branding.corner_radius === 'number') {
    const radius = Math.max(0, Math.min(28, branding.corner_radius))
    root.style.setProperty('--radius-card', `${radius}px`)
  }

  if (branding.font_family && branding.font_family.trim()) {
    // Always keep a system fallback: a font the viewer doesn't have shouldn't
    // drop them to Times New Roman.
    root.style.setProperty(
      '--font-sans',
      `${branding.font_family.trim()}, 'Inter var', 'Inter', ui-sans-serif, system-ui, sans-serif`,
    )
  } else {
    root.style.removeProperty('--font-sans')
  }

  if (branding.theme_mode === 'dark' || branding.theme_mode === 'light' || branding.theme_mode === 'system') {
    instanceDefault = branding.theme_mode
  }

  applyCustomCss(branding.custom_css ?? '')
  applyTheme()
}

/**
 * Inject the operator's CSS.
 *
 * Set as `textContent`, never as markup, so the content cannot close the style
 * element and become HTML. Only an operator can set this, and they already
 * control the server serving the page, so this adds no trust boundary — but it
 * should still not be a way to inject script.
 */
function applyCustomCss(css: string) {
  let element = document.getElementById(CUSTOM_CSS_ID) as HTMLStyleElement | null

  if (!css.trim()) {
    element?.remove()
    return
  }
  if (!element) {
    element = document.createElement('style')
    element.id = CUSTOM_CSS_ID
    document.head.appendChild(element)
  }
  element.textContent = css
}

/** The subset of branding that arrives from the unauthenticated meta endpoint. */
export function applyMetaBranding(meta: InstanceMeta) {
  applyBranding({
    accent_color: meta.accent_color,
    surface_tint: meta.surface_tint,
    corner_radius: meta.corner_radius,
    font_family: meta.font_family,
    custom_css: meta.custom_css,
    theme_mode: meta.theme_mode,
  })
}
