import {
  Bell, Download, Headphones, Image as ImageIcon, Info, KeyRound, LogOut, Monitor, Moon,
  Palette, Smartphone, Sparkles, Sun, User, X,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError, setToken } from '../lib/api'
import { formatBytes } from '../lib/format'
import type { DesktopRelease } from '../lib/types'
import { useStore } from '../lib/store'
import { clearThemeOverride, setTheme, storedTheme, type ThemeChoice } from '../lib/theme'
import Select from './Select'
import NotificationSettings from './settings/NotificationSettings'
import VoiceSettings from './settings/VoiceSettings'
import { Avatar, ColorField, Field, Modal, Switch, toast, useConfirm } from './ui'

type Tab = 'profile' | 'notifications' | 'voice' | 'appearance' | 'account' | 'about'

export default function SettingsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const me = useStore((s) => s.me)
  const instance = useStore((s) => s.instance)
  const patchMe = useStore((s) => s.patchMe)
  const logout = useStore((s) => s.logout)
  const confirm = useConfirm()

  const [tab, setTab] = useState<Tab>('profile')
  const [saving, setSaving] = useState(false)

  const [displayName, setDisplayName] = useState('')
  const [pronouns, setPronouns] = useState('')
  const [bio, setBio] = useState('')
  const [favoriteGame, setFavoriteGame] = useState('')
  const [customStatus, setCustomStatus] = useState('')
  const [accent, setAccent] = useState('#5b6ee8')
  const [avatarUrl, setAvatarUrl] = useState<string | null>(null)
  const [bannerUrl, setBannerUrl] = useState<string | null>(null)
  const [email, setEmail] = useState('')

  const [theme, setThemeState] = useState<ThemeChoice | null>(storedTheme)
  const [compact, setCompact] = useState(() => localStorage.getItem('minichat.compact') === '1')

  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')

  useEffect(() => {
    if (!open || !me) return
    setDisplayName(me.display_name)
    setPronouns(me.pronouns)
    setBio(me.bio)
    setFavoriteGame(me.favorite_game)
    setCustomStatus(me.custom_status)
    setAccent(me.accent_color)
    setAvatarUrl(me.avatar_url)
    setBannerUrl(me.banner_url)
    setEmail(me.email ?? '')
  }, [open, me])

  if (!me) return null

  const saveProfile = async () => {
    setSaving(true)
    try {
      const updated = await api.updateMe({
        display_name: displayName,
        pronouns,
        bio,
        favorite_game: favoriteGame,
        custom_status: customStatus,
        accent_color: accent,
        avatar_url: avatarUrl ?? '',
        banner_url: bannerUrl ?? '',
      })
      patchMe(updated)
      toast.success('Profile saved.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save your profile.')
    } finally {
      setSaving(false)
    }
  }

  const uploadImage = async (file: File, set: (url: string) => void) => {
    try {
      const attachment = await api.upload(file)
      set(attachment.url)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Upload failed.')
    }
  }

  const changePassword = async () => {
    setSaving(true)
    try {
      const { token } = await api.changePassword(currentPassword, newPassword)
      setToken(token)
      setCurrentPassword('')
      setNewPassword('')
      toast.success('Password changed. Other devices were signed out.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not change your password.')
    } finally {
      setSaving(false)
    }
  }

  const TABS: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'profile', label: 'Profile', icon: <User size={15} /> },
    { key: 'notifications', label: 'Notifications', icon: <Bell size={15} /> },
    { key: 'voice', label: 'Voice', icon: <Headphones size={15} /> },
    { key: 'appearance', label: 'Appearance', icon: <Palette size={15} /> },
    { key: 'account', label: 'Account', icon: <KeyRound size={15} /> },
    { key: 'about', label: 'Apps', icon: <Info size={15} /> },
  ]

  return (
    <Modal open={open} onClose={onClose} width="lg" bare>
      <div className="flex flex-col sm:flex-row min-h-[520px]">
        {/* Phones get a picker and a close button that stays put, rather than
            a scroller with the X hidden past the right edge. */}
        <header
          className="sm:hidden flex items-center gap-2 p-3 border-b shrink-0"
          style={{ background: 'var(--surface-0)' }}
        >
          <Select
            value={tab}
            onChange={(next) => setTab(next as Tab)}
            ariaLabel="Settings section"
            options={TABS.map((entry) => ({ value: entry.key, label: entry.label }))}
          />
          <button className="btn btn-ghost !p-2 shrink-0" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </header>

        <nav
          className="hidden sm:w-48 shrink-0 p-3 sm:border-r sm:flex sm:flex-col gap-1"
          style={{ background: 'var(--surface-0)' }}
        >
          <div className="flex items-center justify-between mb-2 px-1">
            <span className="label !mb-0">Settings</span>
            <button className="btn btn-ghost !p-1" onClick={onClose} aria-label="Close">
              <X size={15} />
            </button>
          </div>
          {TABS.map((entry) => (
            <button
              key={entry.key}
              onClick={() => setTab(entry.key)}
              className="flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm transition-colors shrink-0"
              style={{
                background: tab === entry.key ? 'var(--accent-soft)' : 'transparent',
                color: tab === entry.key ? 'var(--accent)' : 'var(--text-muted)',
                fontWeight: tab === entry.key ? 600 : 500,
              }}
            >
              {entry.icon}
              {entry.label}
            </button>
          ))}
        </nav>

        <div className="flex-1 p-5 sm:p-6 overflow-y-auto scroll-thin min-w-0">
          {tab === 'profile' && (
            <div className="space-y-5 animate-fade-in">
              <h2 className="text-lg font-semibold">Your profile</h2>

              <div
                className="rounded-xl overflow-hidden border relative"
                style={{
                  height: 92,
                  background: bannerUrl
                    ? `url(${bannerUrl}) center/cover`
                    : `linear-gradient(135deg, ${accent}, color-mix(in oklab, ${accent} 35%, var(--surface-3)))`,
                }}
              >
                <label className="absolute top-2 right-2 btn btn-subtle !py-1 !px-2 !text-xs cursor-pointer">
                  <ImageIcon size={12} /> Banner
                  <input
                    type="file"
                    accept="image/*"
                    className="hidden"
                    onChange={(event) => {
                      const file = event.target.files?.[0]
                      if (file) void uploadImage(file, setBannerUrl)
                    }}
                  />
                </label>
                {bannerUrl && (
                  <button
                    className="absolute top-2 right-20 btn btn-subtle !py-1 !px-1.5"
                    onClick={() => setBannerUrl(null)}
                    aria-label="Remove banner"
                  >
                    <X size={12} />
                  </button>
                )}
              </div>

              <div className="flex items-center gap-4 -mt-8 px-3">
                <div style={{ boxShadow: '0 0 0 4px var(--surface-1)', borderRadius: '50%' }}>
                  <Avatar id={me.id} name={displayName || me.username} src={avatarUrl} accent={accent} size="xl" />
                </div>
                <div className="flex gap-2 mt-6">
                  <label className="btn btn-subtle cursor-pointer">
                    <ImageIcon size={14} /> Change avatar
                    <input
                      type="file"
                      accept="image/*"
                      className="hidden"
                      onChange={(event) => {
                        const file = event.target.files?.[0]
                        if (file) void uploadImage(file, setAvatarUrl)
                      }}
                    />
                  </label>
                  {avatarUrl && (
                    <button className="btn btn-ghost" onClick={() => setAvatarUrl(null)}>
                      Remove
                    </button>
                  )}
                </div>
              </div>

              <div className="grid sm:grid-cols-2 gap-4">
                <Field label="Display name">
                  <input className="input" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
                </Field>
                <Field label="Pronouns" hint="Shown next to your name.">
                  <input
                    className="input"
                    value={pronouns}
                    onChange={(e) => setPronouns(e.target.value)}
                    placeholder="they/them"
                  />
                </Field>
              </div>

              <Field label="Status" hint="A short line shown under your name.">
                <input
                  className="input"
                  value={customStatus}
                  onChange={(e) => setCustomStatus(e.target.value)}
                  placeholder="Building something"
                />
              </Field>

              <Field label="About me">
                <textarea
                  className="input resize-none"
                  rows={3}
                  value={bio}
                  onChange={(e) => setBio(e.target.value)}
                  placeholder="Tell people a bit about yourself"
                />
              </Field>

              <Field label="Favourite game">
                <input
                  className="input"
                  value={favoriteGame}
                  onChange={(e) => setFavoriteGame(e.target.value)}
                  placeholder="Deep Rock Galactic"
                />
              </Field>

              <ColorField label="Your accent colour" value={accent} onChange={setAccent} />

              <div className="flex justify-end pt-1">
                <button className="btn btn-primary" onClick={saveProfile} disabled={saving}>
                  {saving ? 'Saving…' : 'Save profile'}
                </button>
              </div>
            </div>
          )}

          {tab === 'notifications' && <NotificationSettings />}

          {tab === 'voice' && <VoiceSettings />}

          {tab === 'appearance' && (
            <div className="space-y-5 animate-fade-in">
              <h2 className="text-lg font-semibold">Appearance</h2>
              <div>
                <span className="label">Theme</span>
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                  {(
                    [
                      [null, 'Instance', <Sparkles size={16} key="i" />],
                      ['dark', 'Dark', <Moon size={16} key="d" />],
                      ['light', 'Light', <Sun size={16} key="l" />],
                      ['system', 'System', <Monitor size={16} key="s" />],
                    ] as const
                  ).map(([value, label, icon]) => (
                    <button
                      key={label}
                      onClick={() => {
                        setThemeState(value)
                        if (value === null) clearThemeOverride()
                        else setTheme(value)
                      }}
                      className="flex flex-col items-center gap-1.5 py-4 rounded-xl border transition-colors"
                      style={{
                        borderColor: theme === value ? 'var(--accent)' : 'var(--border)',
                        background: theme === value ? 'var(--accent-soft)' : 'transparent',
                        color: theme === value ? 'var(--accent)' : 'var(--text-muted)',
                      }}
                    >
                      {icon}
                      <span className="text-xs font-medium">{label}</span>
                    </button>
                  ))}
                </div>
              </div>

              <Switch
                checked={compact}
                onChange={(value) => {
                  setCompact(value)
                  localStorage.setItem('minichat.compact', value ? '1' : '0')
                  document.documentElement.style.setProperty('--radius-card', value ? '8px' : '14px')
                }}
                label="Compact corners"
                hint="Tighter, squarer surfaces throughout the app."
              />

              <div className="pt-2">
                <p className="label">Instance accent</p>
                <div className="flex items-center gap-3 p-3 rounded-xl" style={{ background: 'var(--surface-2)' }}>
                  <span className="w-9 h-9 rounded-lg shrink-0" style={{ background: instance?.accent_color }} />
                  <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
                    Set by the instance operator. Your own accent only colours your avatar and profile.
                  </p>
                </div>
              </div>
            </div>
          )}

          {tab === 'account' && (
            <div className="space-y-5 animate-fade-in">
              <h2 className="text-lg font-semibold">Account</h2>

              <Field label="Username" hint="Usernames can't be changed after signup.">
                <input className="input" value={`@${me.username}`} disabled />
              </Field>

              <Field label="Email" hint="Optional. Lets you sign in with your email as well.">
                <div className="flex gap-2">
                  <input className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
                  <button
                    className="btn btn-subtle shrink-0"
                    disabled={saving || email === (me.email ?? '')}
                    onClick={async () => {
                      setSaving(true)
                      try {
                        patchMe(await api.updateMe({ email }))
                        toast.success('Email updated.')
                      } catch (error) {
                        toast.error(error instanceof ApiError ? error.message : 'Could not update your email.')
                      } finally {
                        setSaving(false)
                      }
                    }}
                  >
                    Save
                  </button>
                </div>
              </Field>

              <div className="pt-2 border-t">
                <h3 className="font-semibold text-sm mt-4 mb-3">Change password</h3>
                <div className="space-y-3">
                  <Field label="Current password">
                    <input
                      className="input"
                      type="password"
                      value={currentPassword}
                      onChange={(e) => setCurrentPassword(e.target.value)}
                      autoComplete="current-password"
                    />
                  </Field>
                  <Field label="New password" hint="At least 8 characters. Other devices get signed out.">
                    <input
                      className="input"
                      type="password"
                      value={newPassword}
                      onChange={(e) => setNewPassword(e.target.value)}
                      autoComplete="new-password"
                    />
                  </Field>
                  <button
                    className="btn btn-subtle"
                    onClick={changePassword}
                    disabled={saving || !currentPassword || newPassword.length < 8}
                  >
                    Change password
                  </button>
                </div>
              </div>

              <div className="pt-4 border-t space-y-2">
                <button
                  className="btn btn-subtle w-full"
                  onClick={async () => {
                    const ok = await confirm({
                      title: 'Sign out everywhere?',
                      body: 'Every other device will be signed out. You stay signed in here.',
                      confirmLabel: 'Sign out other devices',
                    })
                    if (!ok) return
                    try {
                      const { token } = await api.revokeSessions()
                      setToken(token)
                      toast.success('All other sessions were ended.')
                    } catch {
                      toast.error('Could not end the other sessions.')
                    }
                  }}
                >
                  Sign out everywhere else
                </button>
                <button
                  className="btn btn-danger w-full"
                  onClick={async () => {
                    const ok = await confirm({ title: 'Sign out?', confirmLabel: 'Sign out', danger: true })
                    if (ok) {
                      onClose()
                      logout()
                    }
                  }}
                >
                  <LogOut size={15} /> Sign out
                </button>
              </div>
            </div>
          )}

          {tab === 'about' && <AboutTab onClose={onClose} />}
        </div>
      </div>
    </Modal>
  )
}

/**
 * Offers the desktop app, resolved from the newest GitHub release rather than
 * a hardcoded link, so a build is downloadable the moment it is published.
 */
function DesktopDownload({ highlight }: { highlight: boolean }) {
  const [release, setRelease] = useState<DesktopRelease | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api
      .desktopLatest()
      .then(setRelease)
      .catch(() => setRelease(null))
      .finally(() => setLoading(false))
  }, [])

  // An installer, preferring the .msi that most people want.
  const primary =
    release?.assets.find((asset) => asset.kind === 'msi') ?? release?.assets[0] ?? null
  const secondary = release?.assets.find((asset) => asset !== primary) ?? null

  return (
    <div className="card p-4">
      <div className="flex items-start gap-3">
        <div
          className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
          style={{
            background: highlight ? 'var(--accent-soft)' : 'var(--surface-3)',
            color: highlight ? 'var(--accent)' : 'var(--text-muted)',
          }}
        >
          <Monitor size={17} />
        </div>
        <div className="min-w-0 flex-1">
          <h3 className="font-semibold text-sm">Windows desktop app</h3>
          <p className="text-xs mt-0.5 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
            A native window with global push-to-talk that works while you're in a game, plus a tray
            icon and no browser permission prompts.
          </p>

          {loading ? (
            <p className="text-xs mt-3" style={{ color: 'var(--text-faint)' }}>
              Checking for the latest build…
            </p>
          ) : release?.available && primary ? (
            <>
              <div className="flex gap-2 mt-3 flex-wrap">
                <a className="btn btn-primary" href={primary.url}>
                  <Download size={14} /> Download {primary.kind === 'msi' ? 'installer' : 'setup'}
                </a>
                {secondary && (
                  <a className="btn btn-subtle" href={secondary.url}>
                    .{secondary.kind}
                  </a>
                )}
              </div>
              <p className="text-[0.7rem] mt-2" style={{ color: 'var(--text-faint)' }}>
                {release.version} · {formatBytes(primary.size)} ·{' '}
                <a
                  href={release.release_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{ color: 'var(--accent)' }}
                >
                  release notes
                </a>
              </p>
            </>
          ) : (
            <p className="text-xs mt-3 leading-relaxed" style={{ color: 'var(--text-faint)' }}>
              No published build found for <code>{release?.repo ?? 'this instance'}</code>. Your
              operator can publish one by tagging a release, or point
              <code className="mx-1">DESKTOP_RELEASE_REPO</code> at their own fork.
            </p>
          )}
        </div>
      </div>
    </div>
  )
}

function AboutTab({ onClose }: { onClose: () => void }) {
  const meta = useStore((s) => s.meta)
  const instance = useStore((s) => s.instance)
  const [installPrompt, setInstallPrompt] = useState<any>(null)
  const [installed, setInstalled] = useState(
    () => window.matchMedia('(display-mode: standalone)').matches,
  )

  useEffect(() => {
    const onPrompt = (event: Event) => {
      event.preventDefault()
      setInstallPrompt(event)
    }
    const onInstalled = () => {
      setInstalled(true)
      setInstallPrompt(null)
    }
    window.addEventListener('beforeinstallprompt', onPrompt)
    window.addEventListener('appinstalled', onInstalled)
    return () => {
      window.removeEventListener('beforeinstallprompt', onPrompt)
      window.removeEventListener('appinstalled', onInstalled)
    }
  }, [])

  const isWindows = navigator.userAgent.includes('Windows')
  const isDesktopApp = navigator.userAgent.includes('MiniChat')

  return (
    <div className="space-y-5 animate-fade-in">
      <h2 className="text-lg font-semibold">Get the app</h2>

      {!isDesktopApp && (
        <div className="card p-4">
          <div className="flex items-start gap-3">
            <div
              className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
              style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
            >
              <Smartphone size={17} />
            </div>
            <div className="min-w-0 flex-1">
              <h3 className="font-semibold text-sm">Install on this device</h3>
              <p className="text-xs mt-0.5 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                {installed
                  ? "You're already running the installed app."
                  : installPrompt
                    ? 'Adds MiniChat to your home screen or dock, with its own window and notifications.'
                    : 'Use your browser menu — "Add to Home Screen" on iOS, or "Install" in the address bar on desktop.'}
              </p>
              {installPrompt && !installed && (
                <button
                  className="btn btn-primary mt-3"
                  onClick={async () => {
                    installPrompt.prompt()
                    const { outcome } = await installPrompt.userChoice
                    if (outcome === 'accepted') {
                      setInstalled(true)
                      onClose()
                    }
                    setInstallPrompt(null)
                  }}
                >
                  <Download size={14} /> Install app
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {!isDesktopApp && <DesktopDownload highlight={isWindows} />}

      <div className="pt-4 border-t text-xs space-y-1" style={{ color: 'var(--text-faint)' }}>
        <p>
          <strong style={{ color: 'var(--text-muted)' }}>{instance?.name}</strong> · MiniChat v{meta?.version}
        </p>
        <p>Voice &amp; video: {meta?.voice_enabled ? 'enabled' : 'not configured'}</p>
      </div>
    </div>
  )
}
