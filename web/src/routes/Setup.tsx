import {
  ArrowLeft, ArrowRight, Check, Hash, Image as ImageIcon, KeyRound, Loader2, Megaphone,
  Plus, Shield, Sparkles, Trash2, Volume2, X,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError } from '../lib/api'
import { P } from '../lib/perms'
import { applyAccent, useStore } from '../lib/store'
import type { InstanceMeta } from '../lib/types'
import { ColorField, Field, Switch, toast } from '../components/ui'

type ChannelDraft = { name: string; kind: 'text' | 'voice' | 'announcement'; topic: string; category: string }
type RoleDraft = { name: string; color: string; permissions: number; hoist: boolean }

const STEPS = ['Welcome', 'Operator', 'Branding', 'Community', 'Roles', 'Channels', 'Review'] as const

const DEFAULT_MEMBER_PERMS = Number(
  P.VIEW_CHANNELS | P.SEND_MESSAGES | P.ATTACH_FILES | P.ADD_REACTIONS | P.CONNECT | P.SPEAK | P.VIDEO | P.SCREEN_SHARE,
)

const MEMBER_TOGGLES: { key: string; bit: bigint; label: string }[] = [
  { key: 'send', bit: P.SEND_MESSAGES, label: 'Send messages' },
  { key: 'files', bit: P.ATTACH_FILES, label: 'Attach files' },
  { key: 'react', bit: P.ADD_REACTIONS, label: 'Add reactions' },
  { key: 'connect', bit: P.CONNECT, label: 'Join voice channels' },
  { key: 'speak', bit: P.SPEAK, label: 'Speak in voice' },
  { key: 'video', bit: P.VIDEO, label: 'Share camera' },
  { key: 'screen', bit: P.SCREEN_SHARE, label: 'Share screen' },
  { key: 'invite', bit: P.CREATE_INVITES, label: 'Create invite links' },
]

const ROLE_PRESETS: RoleDraft[] = [
  {
    name: 'Moderator',
    color: '#34d399',
    hoist: true,
    permissions: Number(
      P.VIEW_CHANNELS | P.SEND_MESSAGES | P.MANAGE_MESSAGES | P.PIN_MESSAGES | P.ATTACH_FILES |
        P.ADD_REACTIONS | P.CONNECT | P.SPEAK | P.VIDEO | P.SCREEN_SHARE | P.MUTE_MEMBERS |
        P.MOVE_MEMBERS | P.KICK_MEMBERS | P.CREATE_INVITES | P.VIEW_AUDIT_LOG,
    ),
  },
  {
    name: 'Admin',
    color: '#f97362',
    hoist: true,
    permissions: Number(P.ADMINISTRATOR),
  },
]

export default function Setup({ meta }: { meta: InstanceMeta }) {
  const [step, setStep] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const finishAuth = useStore((s) => s.finishAuth)

  // Operator
  const [username, setUsername] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [setupToken, setSetupToken] = useState('')

  // Branding
  const [instanceName, setInstanceName] = useState('')
  const [tagline, setTagline] = useState('')
  const [description, setDescription] = useState('')
  const [accent, setAccent] = useState('#5b6ee8')
  const [iconFile, setIconFile] = useState<File | null>(null)
  const [bannerFile, setBannerFile] = useState<File | null>(null)

  // Community
  const [rules, setRules] = useState(
    'Be respectful.\nNo spam or harassment.\nKeep conversations in the right channels.',
  )
  const [welcome, setWelcome] = useState('')
  const [registrationMode, setRegistrationMode] = useState<'invite' | 'open' | 'closed'>('invite')
  const [requireRules, setRequireRules] = useState(true)

  // Roles
  const [defaultRoleName, setDefaultRoleName] = useState('Member')
  const [defaultPerms, setDefaultPerms] = useState(DEFAULT_MEMBER_PERMS)
  const [roles, setRoles] = useState<RoleDraft[]>([ROLE_PRESETS[0]])

  // Channels
  const [channels, setChannels] = useState<ChannelDraft[]>([
    { name: 'announcements', kind: 'announcement', topic: 'Instance news and updates', category: 'Information' },
    { name: 'general', kind: 'text', topic: 'Say hello', category: 'Text' },
    { name: 'off-topic', kind: 'text', topic: 'Anything goes', category: 'Text' },
    { name: 'General', kind: 'voice', topic: '', category: 'Voice' },
  ])

  useEffect(() => applyAccent(accent), [accent])

  const iconPreview = useMemo(() => (iconFile ? URL.createObjectURL(iconFile) : null), [iconFile])
  const bannerPreview = useMemo(() => (bannerFile ? URL.createObjectURL(bannerFile) : null), [bannerFile])
  useEffect(
    () => () => {
      if (iconPreview) URL.revokeObjectURL(iconPreview)
      if (bannerPreview) URL.revokeObjectURL(bannerPreview)
    },
    [iconPreview, bannerPreview],
  )

  const stepValid = (index: number): string => {
    if (index === 1) {
      if (username.trim().length < 2) return 'Pick a username of at least 2 characters.'
      if (!/^[A-Za-z0-9_.-]+$/.test(username.trim())) return 'Usernames use letters, numbers, dots, dashes and underscores.'
      if (password.length < 8) return 'Your password needs at least 8 characters.'
      if (password !== confirmPassword) return "Those passwords don't match."
      if (meta.setup_token_required && !setupToken.trim()) return 'This instance requires the setup token from your .env file.'
      return ''
    }
    if (index === 2 && instanceName.trim().length < 1) return 'Give your instance a name.'
    if (index === 5 && channels.length === 0) return 'Create at least one channel.'
    return ''
  }

  const next = () => {
    const message = stepValid(step)
    if (message) {
      setError(message)
      return
    }
    setError('')
    setStep((value) => Math.min(value + 1, STEPS.length - 1))
  }

  const back = () => {
    setError('')
    setStep((value) => Math.max(value - 1, 0))
  }

  const submit = async () => {
    setBusy(true)
    setError('')
    try {
      const { token } = await api.setup({
        setup_token: setupToken.trim() || undefined,
        username: username.trim(),
        password,
        display_name: displayName.trim() || undefined,
        email: email.trim() || undefined,
        instance_name: instanceName.trim(),
        tagline: tagline.trim(),
        description: description.trim(),
        accent_color: accent,
        rules: rules.trim(),
        welcome_message: welcome.trim(),
        registration_mode: registrationMode,
        require_rules_accept: requireRules,
        default_role_name: defaultRoleName.trim() || 'Member',
        default_permissions: defaultPerms,
        roles: roles.map((role) => ({
          name: role.name,
          color: role.color,
          permissions: role.permissions,
          hoist: role.hoist,
        })),
        channels: channels.map((channel) => ({
          name: channel.name,
          kind: channel.kind,
          topic: channel.topic,
          category: channel.category,
        })),
      })

      // The account exists now, so branding images can finally be uploaded.
      localStorage.setItem('minichat.token', token)
      const patch: Record<string, string> = {}
      try {
        if (iconFile) patch.icon_url = (await api.upload(iconFile)).url
        if (bannerFile) patch.banner_url = (await api.upload(bannerFile)).url
        if (Object.keys(patch).length) await api.updateInstance(patch)
      } catch {
        toast.error('Instance created, but the images failed to upload. Add them in the admin panel.')
      }

      await finishAuth(token)
      toast.success(`${instanceName.trim()} is live. Welcome aboard.`)
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.message : 'Setup failed. Please try again.')
      setBusy(false)
    }
  }

  return (
    <div className="min-h-full flex flex-col" style={{ background: 'var(--bg)' }}>
      <BackdropGlow accent={accent} />

      <div className="relative flex-1 flex flex-col items-center justify-center px-4 py-8 sm:py-12">
        <div className="w-full max-w-2xl">
          <Stepper step={step} />

          <div className="card mt-6 overflow-hidden" style={{ boxShadow: 'var(--shadow-lg)' }}>
            <div className="p-6 sm:p-8 min-h-[380px]">
              {step === 0 && <StepWelcome version={meta.version} />}

              {step === 1 && (
                <StepBlock
                  icon={<KeyRound size={18} />}
                  title="Create the operator account"
                  subtitle="This is the highest role on the instance. It can never be locked out."
                >
                  <div className="grid sm:grid-cols-2 gap-4">
                    <Field label="Username" hint="Used to sign in. Letters, numbers, . - _">
                      <input
                        className="input"
                        value={username}
                        onChange={(e) => setUsername(e.target.value)}
                        placeholder="owner"
                        autoFocus
                        autoComplete="username"
                      />
                    </Field>
                    <Field label="Display name" hint="What everyone else sees.">
                      <input
                        className="input"
                        value={displayName}
                        onChange={(e) => setDisplayName(e.target.value)}
                        placeholder={username || 'Your name'}
                      />
                    </Field>
                  </div>
                  <Field label="Email" hint="Optional. Only used for sign-in and account recovery.">
                    <input
                      className="input"
                      type="email"
                      value={email}
                      onChange={(e) => setEmail(e.target.value)}
                      placeholder="you@example.com"
                      autoComplete="email"
                    />
                  </Field>
                  <div className="grid sm:grid-cols-2 gap-4">
                    <Field label="Password" hint="At least 8 characters.">
                      <input
                        className="input"
                        type="password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        autoComplete="new-password"
                      />
                    </Field>
                    <Field label="Confirm password">
                      <input
                        className="input"
                        type="password"
                        value={confirmPassword}
                        onChange={(e) => setConfirmPassword(e.target.value)}
                        autoComplete="new-password"
                      />
                    </Field>
                  </div>
                  {meta.setup_token_required && (
                    <Field
                      label="Setup token"
                      hint="Your instance sets SETUP_TOKEN in .env — paste it here to prove you're the owner."
                    >
                      <input
                        className="input font-mono"
                        value={setupToken}
                        onChange={(e) => setSetupToken(e.target.value)}
                        spellCheck={false}
                      />
                    </Field>
                  )}
                </StepBlock>
              )}

              {step === 2 && (
                <StepBlock
                  icon={<Sparkles size={18} />}
                  title="Name and brand your instance"
                  subtitle="The accent colour retints the entire app, for everyone."
                >
                  <Field label="Instance name">
                    <input
                      className="input"
                      value={instanceName}
                      onChange={(e) => setInstanceName(e.target.value)}
                      placeholder="The Lounge"
                      autoFocus
                    />
                  </Field>
                  <Field label="Tagline" hint="A short line shown on the invite page.">
                    <input
                      className="input"
                      value={tagline}
                      onChange={(e) => setTagline(e.target.value)}
                      placeholder="A small place for a few good people"
                    />
                  </Field>
                  <Field label="Description">
                    <textarea
                      className="input resize-none"
                      rows={3}
                      value={description}
                      onChange={(e) => setDescription(e.target.value)}
                      placeholder="What is this community for?"
                    />
                  </Field>
                  <ColorField label="Accent colour" value={accent} onChange={setAccent} />
                  <div className="grid sm:grid-cols-2 gap-4">
                    <ImagePicker
                      label="Instance icon"
                      hint="Square works best."
                      preview={iconPreview}
                      onPick={setIconFile}
                      rounded
                    />
                    <ImagePicker
                      label="Banner"
                      hint="Shown on the invite page."
                      preview={bannerPreview}
                      onPick={setBannerFile}
                    />
                  </div>
                </StepBlock>
              )}

              {step === 3 && (
                <StepBlock
                  icon={<Shield size={18} />}
                  title="Rules and who can join"
                  subtitle="You can change any of this later in the admin panel."
                >
                  <Field label="Rules" hint="Shown on the invite page and in the app.">
                    <textarea
                      className="input resize-none font-[inherit]"
                      rows={5}
                      value={rules}
                      onChange={(e) => setRules(e.target.value)}
                    />
                  </Field>
                  <Field label="Welcome message" hint="Greets new members when they first arrive.">
                    <input
                      className="input"
                      value={welcome}
                      onChange={(e) => setWelcome(e.target.value)}
                      placeholder="Glad you made it — introduce yourself in #general!"
                    />
                  </Field>
                  <div>
                    <span className="label">Who can create an account</span>
                    <div className="grid gap-2">
                      {(
                        [
                          ['invite', 'Invite only', 'Members need an invite link. Recommended.'],
                          ['open', 'Open', 'Anyone who can reach the URL can sign up.'],
                          ['closed', 'Closed', 'No new accounts at all.'],
                        ] as const
                      ).map(([value, label, hint]) => (
                        <button
                          key={value}
                          type="button"
                          onClick={() => setRegistrationMode(value)}
                          className="flex items-start gap-3 p-3 rounded-xl border text-left transition-colors"
                          style={{
                            borderColor: registrationMode === value ? 'var(--accent)' : 'var(--border)',
                            background: registrationMode === value ? 'var(--accent-soft)' : 'transparent',
                          }}
                        >
                          <span
                            className="w-4 h-4 rounded-full border-2 mt-0.5 shrink-0 flex items-center justify-center"
                            style={{ borderColor: registrationMode === value ? 'var(--accent)' : 'var(--border)' }}
                          >
                            {registrationMode === value && (
                              <span className="w-2 h-2 rounded-full" style={{ background: 'var(--accent)' }} />
                            )}
                          </span>
                          <span>
                            <span className="text-sm font-medium block">{label}</span>
                            <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
                              {hint}
                            </span>
                          </span>
                        </button>
                      ))}
                    </div>
                  </div>
                  <Switch
                    checked={requireRules}
                    onChange={setRequireRules}
                    label="Require new members to accept the rules"
                    hint="They'll see them before their account is created."
                  />
                </StepBlock>
              )}

              {step === 4 && (
                <StepBlock
                  icon={<Shield size={18} />}
                  title="Roles and permissions"
                  subtitle="Every member gets the default role. Extra roles stack on top."
                >
                  <Field label="Default role name">
                    <input
                      className="input"
                      value={defaultRoleName}
                      onChange={(e) => setDefaultRoleName(e.target.value)}
                    />
                  </Field>
                  <div>
                    <span className="label">What can everyone do?</span>
                    <div className="grid sm:grid-cols-2 gap-2.5 p-3 rounded-xl" style={{ background: 'var(--surface-2)' }}>
                      {MEMBER_TOGGLES.map((toggle) => {
                        const on = (BigInt(defaultPerms) & toggle.bit) !== 0n
                        return (
                          <Switch
                            key={toggle.key}
                            checked={on}
                            label={toggle.label}
                            onChange={(value) =>
                              setDefaultPerms(
                                Number(value ? BigInt(defaultPerms) | toggle.bit : BigInt(defaultPerms) & ~toggle.bit),
                              )
                            }
                          />
                        )
                      })}
                    </div>
                  </div>

                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="label !mb-0">Additional roles</span>
                      <div className="flex gap-1.5">
                        {ROLE_PRESETS.filter((preset) => !roles.some((r) => r.name === preset.name)).map((preset) => (
                          <button
                            key={preset.name}
                            className="btn btn-subtle !py-1 !px-2 !text-xs"
                            onClick={() => setRoles((value) => [...value, preset])}
                          >
                            <Plus size={12} /> {preset.name}
                          </button>
                        ))}
                      </div>
                    </div>
                    <div className="space-y-2">
                      {roles.map((role, index) => (
                        <div
                          key={index}
                          className="flex items-center gap-2 p-2.5 rounded-xl"
                          style={{ background: 'var(--surface-2)' }}
                        >
                          <label className="w-7 h-7 rounded-lg shrink-0 cursor-pointer" style={{ background: role.color }}>
                            <input
                              type="color"
                              className="opacity-0 w-full h-full cursor-pointer"
                              value={role.color}
                              onChange={(e) =>
                                setRoles((value) =>
                                  value.map((r, i) => (i === index ? { ...r, color: e.target.value } : r)),
                                )
                              }
                            />
                          </label>
                          <input
                            className="input !py-1.5"
                            value={role.name}
                            onChange={(e) =>
                              setRoles((value) => value.map((r, i) => (i === index ? { ...r, name: e.target.value } : r)))
                            }
                          />
                          <button
                            className="btn btn-ghost !p-1.5 shrink-0"
                            onClick={() => setRoles((value) => value.filter((_, i) => i !== index))}
                            aria-label={`Remove ${role.name}`}
                          >
                            <Trash2 size={15} />
                          </button>
                        </div>
                      ))}
                      {!roles.length && (
                        <p className="text-sm py-3 text-center" style={{ color: 'var(--text-faint)' }}>
                          No extra roles. You can add them any time.
                        </p>
                      )}
                    </div>
                  </div>
                </StepBlock>
              )}

              {step === 5 && (
                <StepBlock
                  icon={<Hash size={18} />}
                  title="Set up your channels"
                  subtitle="Group them into categories — you can rearrange everything later."
                >
                  <ChannelBuilder channels={channels} onChange={setChannels} />
                </StepBlock>
              )}

              {step === 6 && (
                <StepBlock
                  icon={<Check size={18} />}
                  title="Ready to launch"
                  subtitle="Everything below can be changed later in the admin panel."
                >
                  <Review
                    rows={[
                      ['Instance', instanceName || '—'],
                      ['Operator', `@${username || '—'}`],
                      ['Accent', accent],
                      ['Joining', { invite: 'Invite only', open: 'Open to anyone', closed: 'Closed' }[registrationMode]],
                      ['Default role', defaultRoleName || 'Member'],
                      ['Extra roles', roles.length ? roles.map((r) => r.name).join(', ') : 'None'],
                      ['Channels', `${channels.length} across ${new Set(channels.map((c) => c.category).filter(Boolean)).size || 1} categories`],
                    ]}
                  />
                </StepBlock>
              )}
            </div>

            {error && (
              <div
                className="px-6 sm:px-8 py-3 text-sm border-t"
                style={{ color: 'var(--danger)', background: 'color-mix(in oklab, var(--danger) 8%, transparent)' }}
              >
                {error}
              </div>
            )}

            <footer className="px-6 sm:px-8 py-4 border-t flex items-center justify-between gap-3">
              <button className="btn btn-ghost" onClick={back} disabled={step === 0 || busy}>
                <ArrowLeft size={16} /> Back
              </button>
              <span className="text-xs" style={{ color: 'var(--text-faint)' }}>
                Step {step + 1} of {STEPS.length}
              </span>
              {step < STEPS.length - 1 ? (
                <button className="btn btn-primary" onClick={next}>
                  Continue <ArrowRight size={16} />
                </button>
              ) : (
                <button className="btn btn-primary" onClick={submit} disabled={busy}>
                  {busy ? <Loader2 size={16} className="animate-spin" /> : <Check size={16} />}
                  {busy ? 'Creating…' : 'Create instance'}
                </button>
              )}
            </footer>
          </div>
        </div>
      </div>
    </div>
  )
}

function Stepper({ step }: { step: number }) {
  return (
    <div className="flex items-center gap-1.5">
      {STEPS.map((label, index) => (
        <div key={label} className="flex-1 min-w-0">
          <div
            className="h-1 rounded-full transition-colors"
            style={{ background: index <= step ? 'var(--accent)' : 'var(--surface-3)' }}
          />
          <span
            className="text-[0.68rem] mt-1.5 hidden sm:block truncate font-medium"
            style={{ color: index === step ? 'var(--accent)' : 'var(--text-faint)' }}
          >
            {label}
          </span>
        </div>
      ))}
    </div>
  )
}

function StepBlock({
  icon,
  title,
  subtitle,
  children,
}: {
  icon: React.ReactNode
  title: string
  subtitle: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-5 animate-fade-up">
      <header className="flex items-start gap-3">
        <div
          className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
          style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
        >
          {icon}
        </div>
        <div>
          <h1 className="text-lg font-semibold leading-tight">{title}</h1>
          <p className="text-sm mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {subtitle}
          </p>
        </div>
      </header>
      <div className="space-y-4">{children}</div>
    </div>
  )
}

function StepWelcome({ version }: { version: string }) {
  return (
    <div className="text-center py-6 animate-fade-up">
      <div
        className="w-16 h-16 rounded-2xl mx-auto flex items-center justify-center mb-5"
        style={{ background: 'var(--accent)', boxShadow: '0 12px 40px -12px var(--accent-glow)' }}
      >
        <Sparkles size={28} style={{ color: 'var(--accent-ink)' }} />
      </div>
      <h1 className="text-2xl font-semibold">Welcome to MiniChat</h1>
      <p className="mt-2 text-sm max-w-md mx-auto" style={{ color: 'var(--text-muted)' }}>
        You're about to set up your own community: text channels, voice and video rooms, roles and
        invites — all running on your own server.
      </p>
      <div className="grid sm:grid-cols-3 gap-3 mt-7 text-left">
        {[
          ['Own the data', 'Everything lives in a SQLite file on your machine.'],
          ['Invite who you want', 'Accounts are created from invite links you control.'],
          ['Voice and video', 'Powered by LiveKit over your own HTTPS domain.'],
        ].map(([title, body]) => (
          <div key={title} className="p-3.5 rounded-xl" style={{ background: 'var(--surface-2)' }}>
            <h3 className="text-sm font-semibold">{title}</h3>
            <p className="text-xs mt-1 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
              {body}
            </p>
          </div>
        ))}
      </div>
      <p className="text-xs mt-6" style={{ color: 'var(--text-faint)' }}>
        MiniChat v{version} · this wizard runs only once
      </p>
    </div>
  )
}

function ImagePicker({
  label,
  hint,
  preview,
  onPick,
  rounded,
}: {
  label: string
  hint: string
  preview: string | null
  onPick: (file: File | null) => void
  rounded?: boolean
}) {
  return (
    <Field label={label} hint={hint}>
      <label
        className="relative flex items-center justify-center border border-dashed rounded-xl cursor-pointer overflow-hidden transition-colors hover:border-[var(--accent)]"
        style={{ height: 96, background: 'var(--surface-0)' }}
      >
        {preview ? (
          <>
            <img
              src={preview}
              alt=""
              className={`object-cover ${rounded ? 'w-16 h-16 rounded-xl' : 'w-full h-full'}`}
            />
            <button
              type="button"
              className="absolute top-1.5 right-1.5 btn btn-subtle !p-1"
              onClick={(event) => {
                event.preventDefault()
                onPick(null)
              }}
              aria-label={`Remove ${label}`}
            >
              <X size={13} />
            </button>
          </>
        ) : (
          <span className="flex flex-col items-center gap-1 text-xs" style={{ color: 'var(--text-faint)' }}>
            <ImageIcon size={18} />
            Choose an image
          </span>
        )}
        <input
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          className="hidden"
          onChange={(event) => onPick(event.target.files?.[0] ?? null)}
        />
      </label>
    </Field>
  )
}

function ChannelBuilder({
  channels,
  onChange,
}: {
  channels: ChannelDraft[]
  onChange: (value: ChannelDraft[]) => void
}) {
  const [name, setName] = useState('')
  const [kind, setKind] = useState<ChannelDraft['kind']>('text')
  const [category, setCategory] = useState('Text')

  const add = () => {
    const trimmed = name.trim()
    if (!trimmed) return
    onChange([...channels, { name: trimmed, kind, topic: '', category: category.trim() }])
    setName('')
  }

  const grouped = channels.reduce<Record<string, { channel: ChannelDraft; index: number }[]>>((acc, channel, index) => {
    const key = channel.category.trim() || 'Uncategorised'
    ;(acc[key] ||= []).push({ channel, index })
    return acc
  }, {})

  return (
    <div className="space-y-4">
      <div className="flex gap-2 flex-wrap sm:flex-nowrap">
        <div className="flex rounded-lg overflow-hidden border shrink-0">
          {(
            [
              ['text', Hash],
              ['voice', Volume2],
              ['announcement', Megaphone],
            ] as const
          ).map(([value, Icon]) => (
            <button
              key={value}
              type="button"
              onClick={() => {
                setKind(value)
                setCategory(value === 'voice' ? 'Voice' : value === 'announcement' ? 'Information' : 'Text')
              }}
              className="px-2.5 py-2 transition-colors"
              style={{
                background: kind === value ? 'var(--accent)' : 'var(--surface-2)',
                color: kind === value ? 'var(--accent-ink)' : 'var(--text-muted)',
              }}
              title={value}
            >
              <Icon size={15} />
            </button>
          ))}
        </div>
        <input
          className="input"
          placeholder={kind === 'voice' ? 'Lounge' : 'new-channel'}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), add())}
        />
        <input
          className="input sm:!w-40"
          placeholder="Category"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        />
        <button className="btn btn-primary shrink-0" onClick={add} disabled={!name.trim()}>
          <Plus size={15} />
        </button>
      </div>

      <div className="space-y-3 max-h-64 overflow-y-auto scroll-thin pr-1">
        {Object.entries(grouped).map(([group, entries]) => (
          <div key={group}>
            <p className="text-[0.68rem] font-bold uppercase tracking-wider mb-1.5" style={{ color: 'var(--text-faint)' }}>
              {group}
            </p>
            <div className="space-y-1">
              {entries.map(({ channel, index }) => (
                <div
                  key={index}
                  className="flex items-center gap-2 px-2.5 py-2 rounded-lg group"
                  style={{ background: 'var(--surface-2)' }}
                >
                  {channel.kind === 'voice' ? (
                    <Volume2 size={15} style={{ color: 'var(--text-faint)' }} />
                  ) : channel.kind === 'announcement' ? (
                    <Megaphone size={15} style={{ color: 'var(--text-faint)' }} />
                  ) : (
                    <Hash size={15} style={{ color: 'var(--text-faint)' }} />
                  )}
                  <span className="text-sm flex-1 min-w-0 truncate">{channel.name}</span>
                  <button
                    className="btn btn-ghost !p-1 opacity-0 group-hover:opacity-100 transition-opacity"
                    onClick={() => onChange(channels.filter((_, i) => i !== index))}
                    aria-label={`Remove ${channel.name}`}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

function Review({ rows }: { rows: [string, string][] }) {
  return (
    <div className="rounded-xl overflow-hidden border">
      {rows.map(([label, value], index) => (
        <div
          key={label}
          className="flex items-center justify-between gap-4 px-4 py-2.5 text-sm"
          style={{ background: index % 2 ? 'var(--surface-2)' : 'transparent' }}
        >
          <span style={{ color: 'var(--text-muted)' }}>{label}</span>
          <span className="font-medium text-right truncate">{value}</span>
        </div>
      ))}
    </div>
  )
}

export function BackdropGlow({ accent }: { accent: string }) {
  return (
    <div className="fixed inset-0 pointer-events-none overflow-hidden" aria-hidden>
      <div
        className="absolute rounded-full"
        style={{
          width: 620,
          height: 620,
          top: -260,
          left: '50%',
          transform: 'translateX(-50%)',
          background: `radial-gradient(circle, ${accent}22 0%, transparent 68%)`,
        }}
      />
      <div
        className="absolute rounded-full"
        style={{
          width: 460,
          height: 460,
          bottom: -220,
          right: -120,
          background: `radial-gradient(circle, ${accent}18 0%, transparent 70%)`,
        }}
      />
    </div>
  )
}
