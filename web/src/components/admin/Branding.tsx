import { Image as ImageIcon, Save, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { useStore } from '../../lib/store'
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
    })
  }, [instance])

  const update = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) =>
    setForm((current) => ({ ...current, [key]: value }))

  const uploadImage = async (file: File, key: 'icon_url' | 'banner_url') => {
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
            <select
              className="input"
              value={form.default_role_id}
              onChange={(e) => update('default_role_id', e.target.value)}
            >
              {roles.map((role) => (
                <option key={role.id} value={role.id}>
                  {role.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="System channel" hint="Where join messages are posted.">
            <select
              className="input"
              value={form.system_channel_id}
              onChange={(e) => update('system_channel_id', e.target.value)}
            >
              <option value="">Don't post join messages</option>
              {channels
                .filter((channel) => channel.kind !== 'voice')
                .map((channel) => (
                  <option key={channel.id} value={channel.id}>
                    #{channel.name}
                  </option>
                ))}
            </select>
          </Field>
        </div>
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
