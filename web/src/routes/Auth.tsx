import { ArrowRight, Check, Loader2, LogIn, ShieldCheck, Sparkles, UserPlus, Users } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../lib/api'
import { applyAccent, useStore } from '../lib/store'
import type { InstanceMeta, InvitePreview } from '../lib/types'
import { Field, Spinner, toast } from '../components/ui'
import { BackdropGlow } from './Setup'

type Mode = 'login' | 'register'

export default function Auth({ meta, inviteCode }: { meta: InstanceMeta; inviteCode: string | null }) {
  const finishAuth = useStore((s) => s.finishAuth)
  const [mode, setMode] = useState<Mode>(inviteCode ? 'register' : 'login')
  const [invite, setInvite] = useState<InvitePreview | null>(null)
  const [inviteLoading, setInviteLoading] = useState(Boolean(inviteCode))

  const [username, setUsername] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [acceptRules, setAcceptRules] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!inviteCode) return
    let cancelled = false
    api
      .invitePreview(inviteCode)
      .then((preview) => {
        if (cancelled) return
        setInvite(preview)
        applyAccent(preview.accent_color)
      })
      .catch(() => {
        if (!cancelled) setInvite(null)
      })
      .finally(() => !cancelled && setInviteLoading(false))
    return () => {
      cancelled = true
    }
  }, [inviteCode])

  const branding = invite
    ? {
        name: invite.instance_name,
        tagline: invite.tagline,
        icon: invite.icon_url,
        banner: invite.banner_url,
        accent: invite.accent_color,
        rules: invite.rules,
        requireRules: invite.require_rules_accept,
        members: invite.member_count,
      }
    : {
        name: meta.name,
        tagline: meta.tagline,
        icon: meta.icon_url,
        banner: meta.banner_url,
        accent: meta.accent_color,
        rules: meta.rules,
        requireRules: meta.require_rules_accept,
        members: meta.member_count,
      }

  const registrationBlocked = !inviteCode && meta.registration_mode !== 'open'
  const needsRules = mode === 'register' && branding.requireRules && branding.rules.trim().length > 0

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError('')
    try {
      if (mode === 'login') {
        const { token } = await api.login(username.trim(), password)
        await finishAuth(token)
      } else {
        if (needsRules && !acceptRules) {
          setError('Please accept the rules to continue.')
          setBusy(false)
          return
        }
        const { token } = await api.register({
          username: username.trim(),
          password,
          display_name: displayName.trim() || undefined,
          email: email.trim() || undefined,
          invite: inviteCode ?? undefined,
          accept_rules: acceptRules || !needsRules,
        })
        await finishAuth(token)
        toast.success(`Welcome to ${branding.name}!`)
      }
      // Drop the invite code from the URL so a refresh lands in the app.
      window.history.replaceState({}, '', '/')
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.message : 'Something went wrong. Please try again.')
      setBusy(false)
    }
  }

  if (inviteLoading) {
    return (
      <div className="min-h-full flex items-center justify-center" style={{ color: 'var(--text-muted)' }}>
        <Spinner size={22} />
      </div>
    )
  }

  if (inviteCode && (!invite || !invite.valid)) {
    return (
      <div className="min-h-full flex items-center justify-center p-4" style={{ background: 'var(--bg)' }}>
        <BackdropGlow accent={meta.accent_color} />
        <div className="relative card p-8 text-center max-w-sm w-full">
          <div
            className="w-12 h-12 rounded-xl mx-auto flex items-center justify-center mb-4"
            style={{ background: 'color-mix(in oklab, var(--danger) 16%, transparent)', color: 'var(--danger)' }}
          >
            <ShieldCheck size={22} />
          </div>
          <h1 className="text-lg font-semibold">This invite isn't valid</h1>
          <p className="text-sm mt-2" style={{ color: 'var(--text-muted)' }}>
            {invite?.reason === 'expired'
              ? 'The link has expired. Ask whoever invited you for a fresh one.'
              : invite?.reason === 'used_up'
                ? 'This link has already been used the maximum number of times.'
                : invite?.reason === 'revoked'
                  ? 'This link was revoked by an admin.'
                  : "We couldn't find that invite. Double-check the link."}
          </p>
          <a href="/" className="btn btn-subtle mt-5 w-full">
            Go to sign in
          </a>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-full flex flex-col lg:flex-row" style={{ background: 'var(--bg)' }}>
      <BackdropGlow accent={branding.accent} />

      {/* Instance showcase */}
      <aside className="relative lg:w-[46%] lg:min-h-full flex flex-col justify-center px-6 sm:px-10 py-10 lg:py-16 safe-top">
        <div className="max-w-md mx-auto lg:mx-0 w-full">
          {branding.banner && (
            <div
              className="w-full rounded-2xl overflow-hidden mb-6 border"
              style={{ height: 130, boxShadow: 'var(--shadow-md)' }}
            >
              <img src={branding.banner} alt="" className="w-full h-full object-cover" />
            </div>
          )}

          <div className="flex items-center gap-3.5">
            {branding.icon ? (
              <img
                src={branding.icon}
                alt=""
                className="w-14 h-14 rounded-2xl object-cover border"
                style={{ boxShadow: 'var(--shadow-md)' }}
              />
            ) : (
              <div
                className="w-14 h-14 rounded-2xl flex items-center justify-center shrink-0"
                style={{ background: branding.accent, boxShadow: `0 10px 30px -10px ${branding.accent}88` }}
              >
                <Sparkles size={24} style={{ color: 'var(--accent-ink)' }} />
              </div>
            )}
            <div className="min-w-0">
              <h1 className="text-2xl font-semibold leading-tight truncate">{branding.name}</h1>
              {branding.tagline && (
                <p className="text-sm truncate" style={{ color: 'var(--text-muted)' }}>
                  {branding.tagline}
                </p>
              )}
            </div>
          </div>

          {invite?.inviter && (
            <div
              className="mt-5 flex items-center gap-2.5 px-3.5 py-2.5 rounded-xl text-sm"
              style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
            >
              <UserPlus size={16} className="shrink-0" />
              <span>
                <strong>{invite.inviter}</strong> invited you to join.
              </span>
            </div>
          )}

          <div className="flex items-center gap-4 mt-5 text-sm" style={{ color: 'var(--text-muted)' }}>
            <span className="inline-flex items-center gap-1.5">
              <Users size={15} />
              {branding.members} {branding.members === 1 ? 'member' : 'members'}
            </span>
            {meta.voice_enabled && (
              <span className="inline-flex items-center gap-1.5">
                <Check size={15} style={{ color: 'var(--success)' }} />
                Voice &amp; video
              </span>
            )}
          </div>

          {branding.rules.trim() && (
            <div className="mt-6">
              <h2 className="label">Community rules</h2>
              <div
                className="text-sm rounded-xl p-4 space-y-1.5 whitespace-pre-wrap leading-relaxed max-h-56 overflow-y-auto scroll-thin"
                style={{ background: 'var(--surface-1)', color: 'var(--text-muted)' }}
              >
                {branding.rules}
              </div>
            </div>
          )}
        </div>
      </aside>

      {/* Auth form */}
      <main className="relative flex-1 flex items-center justify-center px-6 sm:px-10 py-10 lg:py-16 safe-bottom">
        <form onSubmit={submit} className="card p-6 sm:p-8 w-full max-w-sm" style={{ boxShadow: 'var(--shadow-lg)' }}>
          <h2 className="text-xl font-semibold">
            {mode === 'login' ? 'Welcome back' : inviteCode ? 'Create your account' : 'Join this instance'}
          </h2>
          <p className="text-sm mt-1 mb-6" style={{ color: 'var(--text-muted)' }}>
            {mode === 'login'
              ? `Sign in to ${branding.name}.`
              : 'Pick a username and password — that’s all you need.'}
          </p>

          <div className="space-y-4">
            <Field label={mode === 'login' ? 'Username or email' : 'Username'}>
              <input
                className="input"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete={mode === 'login' ? 'username' : 'off'}
                autoFocus
                required
              />
            </Field>

            {mode === 'register' && (
              <>
                <Field label="Display name" hint="Optional — defaults to your username.">
                  <input
                    className="input"
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    placeholder={username || 'Your name'}
                  />
                </Field>
                <Field label="Email" hint="Optional. Lets you sign in with your email too.">
                  <input
                    className="input"
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    autoComplete="email"
                  />
                </Field>
              </>
            )}

            <Field label="Password" hint={mode === 'register' ? 'At least 8 characters.' : undefined}>
              <input
                className="input"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
                required
              />
            </Field>

            {needsRules && (
              <label className="flex items-start gap-2.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={acceptRules}
                  onChange={(e) => setAcceptRules(e.target.checked)}
                  className="mt-0.5 w-4 h-4 accent-[var(--accent)] shrink-0"
                />
                <span className="text-sm" style={{ color: 'var(--text-muted)' }}>
                  I've read and accept the community rules.
                </span>
              </label>
            )}
          </div>

          {error && (
            <p className="text-sm mt-4" style={{ color: 'var(--danger)' }}>
              {error}
            </p>
          )}

          <button className="btn btn-primary w-full mt-6" disabled={busy}>
            {busy ? <Loader2 size={16} className="animate-spin" /> : mode === 'login' ? <LogIn size={16} /> : <ArrowRight size={16} />}
            {busy ? 'Just a moment…' : mode === 'login' ? 'Sign in' : 'Create account'}
          </button>

          <p className="text-sm text-center mt-5" style={{ color: 'var(--text-muted)' }}>
            {mode === 'login' ? (
              registrationBlocked ? (
                <span style={{ color: 'var(--text-faint)' }}>
                  {meta.registration_mode === 'closed'
                    ? 'This instance is not accepting new members.'
                    : 'New accounts need an invite link.'}
                </span>
              ) : (
                <>
                  Don't have an account?{' '}
                  <button type="button" className="font-medium" style={{ color: 'var(--accent)' }} onClick={() => { setMode('register'); setError('') }}>
                    Sign up
                  </button>
                </>
              )
            ) : (
              <>
                Already a member?{' '}
                <button type="button" className="font-medium" style={{ color: 'var(--accent)' }} onClick={() => { setMode('login'); setError('') }}>
                  Sign in
                </button>
              </>
            )}
          </p>
        </form>
      </main>
    </div>
  )
}
