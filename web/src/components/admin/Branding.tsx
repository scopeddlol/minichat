import { Image as ImageIcon, Monitor, Moon, Save, Sun, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { useStore } from '../../lib/store'
import Select from '../Select'
import { ColorField, Field, Switch, toast } from '../ui'

export default function Branding() {
  const instance = useStore((s) => s.instance)
  const channels = useStore((s) => s.channels)
  const roles = useStore((s) => s.roles)

  const [form, setForm] = useState({
    name: '',
    tagline: '',
    description: '',
    accent_color: '#5b6ee8',
    rules: '',
    welcome_message: '',
    registration_mode: 'invite',
    require_rules_accept: true,
    icon_url: '' as string | null,
    banner_url: '' as string | null,
    default_role_id: '',
    system_channel_id: '',
    max_upload_mb: 25,
    theme_mode: 'dark',
    surface_tint: '' as string,
    corner_radius: 14,
    font_family: '',
    custom_css: '',
    login_headline: '',
    login_body: '',
    login_image_url: '' as string | null,
  })
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!instance) return
    setForm({
      name: instance.name,
      tagline: instance.tagline,
      description: instance.description,
      accent_color: instance.accent_color,
      rules: instance.rules,
      welcome_message: instance.welcome_message,
      registration_mode: instance.registration_mode,
      require_rules_accept: instance.require_rules_accept,
      icon_url: instance.icon_url,
      banner_url: instance.banner_url,
      default_role_id: instance.default_role_id ?? '',
      system_channel_id: instance.system_channel_id ?? '',
      max_upload_mb: instance.max_upload_mb,
      theme_mode: instance.theme_mode,
      surface_tint: instance.surface_tint ?? '',
      corner_radius: instance.corner_radius,
      font_family: instance.font_family,
      custom_css: instance.custom_css,
      login_headline: instance.login_headline,
      login_body: instance.login_body,
      login_image_url: instance.login_image_url,
    })
  }, [instance])

  const update = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) =>
    setForm((current) => ({ ...current, [key]: value }))

  const uploadImage = async (file: File, key: 'icon_url' | 'banner_url' | 'login_image_url') => {
    try {
      update(key, (await api.upload(file)).url)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Upload failed.')
    }
  }

  const save = async () => {
    setSaving(true)
    try {
      await api.updateInstance({
        ...form,
        icon_url: form.icon_url ?? '',
        banner_url: form.banner_url ?? '',
        login_image_url: form.login_image_url ?? '',
        system_channel_id: form.system_channel_id || null,
      })
      toast.success('Instance settings saved.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save.')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-6 animate-fade-in max-w-2xl">
      <section className="card p-5 space-y-4">
        <h3 className="font-semibold">Identity</h3>
        <Field label="Instance name">
          <input className="input" value={form.name} onChange={(e) => update('name', e.target.value)} />
        </Field>
        <Field label="Tagline" hint="A short line shown on the invite page and sidebar.">
          <input className="input" value={form.tagline} onChange={(e) => update('tagline', e.target.value)} />
        </Field>
        <Field label="Description">
          <textarea
            className="input resize-none"
            rows={3}
            value={form.description}
            onChange={(e) => update('description', e.target.value)}
          />
        </Field>
        <ColorField label="Accent colour" value={form.accent_color} onChange={(v) => update('accent_color', v)} />

        <div className="grid sm:grid-cols-2 gap-4">
          <ImageField
            label="Icon"
            value={form.icon_url}
            rounded
            onPick={(file) => void uploadImage(file, 'icon_url')}
            onClear={() => update('icon_url', '')}
          />
          <ImageField
            label="Banner"
            value={form.banner_url}
            onPick={(file) => void uploadImage(file, 'banner_url')}
            onClear={() => update('banner_url', '')}
          />
        </div>
      </section>

      <section className="card p-5 space-y-4">
        <h3 className="font-semibold">Joining</h3>
        <div>
          <span className="label">Who can create an account</span>
          <div className="grid sm:grid-cols-3 gap-2">
            {(
              [
                ['invite', 'Invite only'],
                ['open', 'Open'],
                ['closed', 'Closed'],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                onClick={() => update('registration_mode', value)}
                className="py-2.5 rounded-lg border text-sm font-medium transition-colors"
                style={{
                  borderColor: form.registration_mode === value ? 'var(--accent)' : 'var(--border)',
                  background: form.registration_mode === value ? 'var(--accent-soft)' : 'transparent',
                  color: form.registration_mode === value ? 'var(--accent)' : 'var(--text-muted)',
                }}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        <Field label="Rules" hint="Shown on the invite page before someone joins.">
          <textarea className="input resize-none" rows={5} value={form.rules} onChange={(e) => update('rules', e.target.value)} />
        </Field>

        <Switch
          checked={form.require_rules_accept}
          onChange={(v) => update('require_rules_accept', v)}
          label="Require new members to accept the rules"
        />

        <Field label="Welcome message" hint="Shown at the top of a channel a new member opens.">
          <input
            className="input"
            value={form.welcome_message}
            onChange={(e) => update('welcome_message', e.target.value)}
          />
        </Field>

        <div className="grid sm:grid-cols-2 gap-4">
          <Field label="Default role" hint="Automatically granted to every new member.">
            <Select
              value={form.default_role_id}
              onChange={(next) => update('default_role_id', next)}
              ariaLabel="Default role"
              options={roles.map((role) => ({
                value: role.id,
                label: role.name,
                swatch: role.color,
              }))}
            />
          </Field>
          <Field label="System channel" hint="Where join messages are posted.">
            <Select
              value={form.system_channel_id}
              onChange={(next) => update('system_channel_id', next)}
              ariaLabel="System channel"
              options={[
                { value: '', label: "Don't post join messages" },
                ...channels
                  .filter((channel) => channel.kind !== 'voice')
                  .map((channel) => ({ value: channel.id, label: `#${channel.name}` })),
              ]}
            />
          </Field>
        </div>
      </section>

      <section className="card p-5 space-y-4">
        <h3 className="font-semibold">Theme</h3>
        <p className="text-xs -mt-2" style={{ color: 'var(--text-muted)' }}>
          Applies to everyone who hasn't picked a theme of their own.
        </p>

        <div>
          <span className="label">Default appearance</span>
          <div className="grid grid-cols-3 gap-2">
            {(
              [
                ['dark', 'Dark', <Moon size={15} key="d" />],
                ['light', 'Light', <Sun size={15} key="l" />],
                ['system', 'Follow device', <Monitor size={15} key="s" />],
              ] as const
            ).map(([value, label, icon]) => (
              <button
                key={value}
                onClick={() => update('theme_mode', value)}
                className="flex flex-col items-center gap-1.5 py-3 rounded-xl border text-xs font-medium transition-colors"
                style={{
                  borderColor: form.theme_mode === value ? 'var(--accent)' : 'var(--border)',
                  background: form.theme_mode === value ? 'var(--accent-soft)' : 'transparent',
                  color: form.theme_mode === value ? 'var(--accent)' : 'var(--text-muted)',
                }}
              >
                {icon}
                {label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="label">Surface tint</span>
          <p className="text-xs mb-2" style={{ color: 'var(--text-muted)' }}>
            Casts every panel toward a hue, so the instance reads warm or cool rather than only
            carrying a coloured accent. Leave it off for the neutral palette.
          </p>
          <div className="flex items-center gap-2 flex-wrap">
            <button
              className="btn btn-subtle !py-1.5"
              onClick={() => update('surface_tint', '')}
              style={!form.surface_tint ? { borderColor: 'var(--accent)', color: 'var(--accent)' } : undefined}
            >
              None
            </button>
            {['#5b6ee8', '#8b5cff', '#e8639b', '#f97362', '#34d399', '#22b8cf'].map((tint) => (
              <button
                key={tint}
                onClick={() => update('surface_tint', tint)}
                className="w-8 h-8 rounded-lg border transition-transform hover:scale-110"
                style={{
                  background: tint,
                  borderColor: form.surface_tint === tint ? 'var(--text)' : 'var(--border)',
                }}
                aria-label={`Tint ${tint}`}
              />
            ))}
            <input
              className="input font-mono !w-28"
              value={form.surface_tint}
              placeholder="#000000"
              onChange={(e) => update('surface_tint', e.target.value)}
            />
          </div>
        </div>

        <Field label={`Corner radius — ${form.corner_radius}px`} hint="0 for sharp corners, 28 for very soft ones.">
          <input
            type="range"
            min={0}
            max={28}
            value={form.corner_radius}
            onChange={(e) => update('corner_radius', Number(e.target.value))}
            className="w-full accent-[var(--accent)]"
          />
        </Field>

        <Field
          label="Font"
          hint="A font family available to your members, or one your custom CSS imports. Blank uses the default."
        >
          <input
            className="input"
            value={form.font_family}
            onChange={(e) => update('font_family', e.target.value)}
            placeholder="Inter"
          />
        </Field>

        <Field
          label="Custom CSS"
          hint="For anything the settings above don't cover. Applies to every member, so test it before saving."
        >
          <textarea
            className="input resize-none font-mono !text-xs"
            rows={6}
            value={form.custom_css}
            onChange={(e) => update('custom_css', e.target.value)}
            placeholder={':root { --accent: #ff6600; }'}
            spellCheck={false}
          />
        </Field>
      </section>

      <section className="card p-5 space-y-4">
        <h3 className="font-semibold">Sign-in &amp; invite pages</h3>
        <p className="text-xs -mt-2" style={{ color: 'var(--text-muted)' }}>
          What people see before they have an account.
        </p>

        <Field label="Headline" hint="Replaces the default greeting above the sign-in form.">
          <input
            className="input"
            value={form.login_headline}
            onChange={(e) => update('login_headline', e.target.value)}
            placeholder="Welcome back"
          />
        </Field>

        <Field label="Body text" hint="A short paragraph under the headline.">
          <textarea
            className="input resize-none"
            rows={3}
            value={form.login_body}
            onChange={(e) => update('login_body', e.target.value)}
            placeholder="Sign in to pick up where you left off."
          />
        </Field>

        <ImageField
          label="Background image"
          value={form.login_image_url}
          onPick={(file) => void uploadImage(file, 'login_image_url')}
          onClear={() => update('login_image_url', '')}
        />
      </section>

      <section className="card p-5 space-y-4">
        <h3 className="font-semibold">Uploads</h3>
        <Field label="Maximum file size (MB)" hint="Applies to every attachment, avatar and banner.">
          <input
            className="input"
            type="number"
            min={1}
            max={500}
            value={form.max_upload_mb}
            onChange={(e) => update('max_upload_mb', Number(e.target.value))}
          />
        </Field>
      </section>

      <div className="flex justify-end sticky bottom-0 py-3" style={{ background: 'var(--bg)' }}>
        <button className="btn btn-primary" onClick={save} disabled={saving}>
          <Save size={15} /> {saving ? 'Saving…' : 'Save changes'}
        </button>
      </div>
    </div>
  )
}

function ImageField({
  label,
  value,
  rounded,
  onPick,
  onClear,
}: {
  label: string
  value: string | null
  rounded?: boolean
  onPick: (file: File) => void
  onClear: () => void
}) {
  return (
    <Field label={label}>
      <label
        className="relative flex items-center justify-center border border-dashed rounded-xl cursor-pointer overflow-hidden hover:border-[var(--accent)] transition-colors"
        style={{ height: 88, background: 'var(--surface-0)' }}
      >
        {value ? (
          <>
            <img src={value} alt="" className={rounded ? 'w-14 h-14 rounded-xl object-cover' : 'w-full h-full object-cover'} />
            <button
              type="button"
              className="absolute top-1.5 right-1.5 btn btn-subtle !p-1"
              onClick={(event) => {
                event.preventDefault()
                onClear()
              }}
              aria-label={`Remove ${label}`}
            >
              <X size={12} />
            </button>
          </>
        ) : (
          <span className="flex flex-col items-center gap-1 text-xs" style={{ color: 'var(--text-faint)' }}>
            <ImageIcon size={17} /> Choose an image
          </span>
        )}
        <input
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(event) => {
            const file = event.target.files?.[0]
            if (file) onPick(file)
          }}
        />
      </label>
    </Field>
  )
}
