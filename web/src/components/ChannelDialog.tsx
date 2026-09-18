import { Lock, Users } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError } from '../lib/api'
import { useStore } from '../lib/store'
import type { Category, Channel, ChannelKind, Role } from '../lib/types'
import Select from './Select'
import { ChannelIcon } from './Sidebar'
import { Field, Modal, Switch, toast } from './ui'

/**
 * Create or edit a channel.
 *
 * One dialog for both so the fields can't drift apart — the old create modal
 * offered four of them and everything else was admin-panel-only, which is why
 * renaming a channel meant opening the admin panel.
 */
export default function ChannelDialog({
  open,
  onClose,
  channel,
  defaultCategoryId,
  defaultKind,
}: {
  open: boolean
  onClose: () => void
  /** Editing an existing channel; omit to create one. */
  channel?: Channel | null
  /** Pre-selected category, set when created from a category's own menu. */
  defaultCategoryId?: string | null
  defaultKind?: ChannelKind
}) {
  const categories = useStore((s) => s.categories)
  const roles = useStore((s) => s.roles)
  const setActiveChannel = useStore((s) => s.setActiveChannel)
  const editing = !!channel

  const [kind, setKind] = useState<ChannelKind>('text')
  const [name, setName] = useState('')
  const [emoji, setEmoji] = useState('')
  const [description, setDescription] = useState('')
  const [topic, setTopic] = useState('')
  const [categoryId, setCategoryId] = useState('')
  const [isPrivate, setIsPrivate] = useState(false)
  const [syncCategory, setSyncCategory] = useState(false)
  const [allowed, setAllowed] = useState<string[]>([])
  const [busy, setBusy] = useState(false)

  // Reset every time the dialog opens, so a cancelled edit doesn't leak into
  // the next one.
  useEffect(() => {
    if (!open) return
    setKind(channel?.kind ?? defaultKind ?? 'text')
    setName(channel?.name ?? '')
    setEmoji(channel?.emoji ?? '')
    setDescription(channel?.description ?? '')
    setTopic(channel?.topic ?? '')
    setCategoryId(channel?.category_id ?? defaultCategoryId ?? '')
    setIsPrivate(channel?.is_private ?? false)
    // A new channel inside a category inherits by default; that is what makes
    // a private category hold.
    setSyncCategory(channel ? channel.sync_category : !!(defaultCategoryId ?? ''))
    setAllowed([])
    if (channel?.is_private) {
      api
        .allowedRoles(channel.id)
        .then((result) => setAllowed(result.role_ids))
        .catch(() => undefined)
    }
  }, [open, channel, defaultCategoryId, defaultKind])

  const category = useMemo(
    () => categories.find((c) => c.id === categoryId) ?? null,
    [categories, categoryId],
  )

  // Inheriting is only offered when there is a category to inherit from.
  const canSync = !!categoryId
  const effectiveSync = canSync && syncCategory
  // While synced, the category owns access, so the channel's own controls
  // would be lying about what takes effect.
  const ownsAccess = !effectiveSync

  const save = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setBusy(true)
    try {
      if (channel) {
        await api.updateChannel(channel.id, {
          name: trimmed,
          emoji,
          description,
          topic,
          category_id: categoryId || null,
          is_private: isPrivate,
          sync_category: effectiveSync,
          ...(ownsAccess ? { allowed_role_ids: allowed } : {}),
        })
        toast.success('Channel saved.')
      } else {
        const created = await api.createChannel({
          name: trimmed,
          kind,
          emoji,
          description,
          topic,
          category_id: categoryId || null,
          is_private: isPrivate,
          sync_category: effectiveSync,
          allowed_role_ids: allowed,
        })
        toast.success(`${created.name} created.`)
        if (created.kind !== 'voice') setActiveChannel(created.id)
      }
      onClose()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save the channel.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={editing ? `Edit ${channel?.name}` : 'Create a channel'}
      description={
        editing
          ? 'Renaming is safe — channels are linked by id, never by name.'
          : 'Names can use spaces and capitals.'
      }
      width="md"
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={save} disabled={busy || !name.trim()}>
            {busy ? 'Saving…' : editing ? 'Save channel' : 'Create channel'}
          </button>
        </>
      }
    >
      <div className="px-5 py-4 space-y-4">
        {!editing && (
          <div>
            <span className="label">Type</span>
            <div className="grid grid-cols-3 gap-2">
              {(
                [
                  ['text', 'Text'],
                  ['voice', 'Voice'],
                  ['announcement', 'Announce'],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  onClick={() => setKind(value)}
                  className="flex flex-col items-center gap-1.5 py-3 rounded-xl border text-xs font-medium transition-colors"
                  style={{
                    borderColor: kind === value ? 'var(--accent)' : 'var(--border)',
                    background: kind === value ? 'var(--accent-soft)' : 'transparent',
                    color: kind === value ? 'var(--accent)' : 'var(--text-muted)',
                  }}
                >
                  <ChannelIcon kind={value} size={17} />
                  {label}
                </button>
              ))}
            </div>
          </div>
        )}

        <div className="flex gap-2">
          <div className="w-20 shrink-0">
            <Field label="Icon" hint="Optional">
              <input
                className="input text-center"
                value={emoji}
                maxLength={8}
                onChange={(event) => setEmoji(event.target.value)}
                placeholder="#"
              />
            </Field>
          </div>
          <div className="flex-1 min-w-0">
            <Field label="Name">
              <input
                className="input"
                value={name}
                autoFocus
                maxLength={48}
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => event.key === 'Enter' && void save()}
                placeholder={kind === 'voice' ? 'Lounge' : 'Game Night'}
              />
            </Field>
          </div>
        </div>

        <Field label="Description" hint="A short line under the name in the sidebar">
          <input
            className="input"
            value={description}
            maxLength={100}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="Where we plan the next session"
          />
        </Field>

        {kind !== 'voice' && (
          <Field label="Topic" hint="The longer text in the channel header">
            <input
              className="input"
              value={topic}
              maxLength={256}
              onChange={(event) => setTopic(event.target.value)}
              placeholder="Anything goes, keep it kind"
            />
          </Field>
        )}

        <Field label="Category">
          <Select
            value={categoryId}
            onChange={(next) => {
              setCategoryId(next)
              // Moving into a category offers inheritance; moving out of one
              // has nothing left to inherit.
              if (!next) setSyncCategory(false)
              else if (!editing) setSyncCategory(true)
            }}
            ariaLabel="Category"
            options={[
              { value: '', label: 'No category' },
              ...categories.map((entry: Category) => ({
                value: entry.id,
                label: entry.name,
                hint: entry.is_private ? 'Private' : undefined,
                icon: entry.is_private ? <Lock size={13} /> : undefined,
              })),
            ]}
          />
        </Field>

        {canSync && (
          <Switch
            checked={syncCategory}
            onChange={setSyncCategory}
            label={`Use ${category?.name ?? 'the category'}'s permissions`}
            hint={
              syncCategory
                ? 'Who can see this channel is decided by the category. Changes there apply here.'
                : 'This channel keeps its own permissions, and ignores the category.'
            }
          />
        )}

        <div className="pt-1 border-t" style={{ borderColor: 'var(--border-soft)' }} />

        <Switch
          checked={effectiveSync ? (category?.is_private ?? false) : isPrivate}
          onChange={setIsPrivate}
          disabled={effectiveSync}
          label="Private channel"
          hint={
            effectiveSync
              ? `Inherited from ${category?.name ?? 'the category'}, which is ${
                  category?.is_private ? 'private' : 'public'
                }.`
              : 'Hidden from everyone except the roles you pick.'
          }
        />

        {ownsAccess && isPrivate && (
          <RolePicker roles={roles} value={allowed} onChange={setAllowed} />
        )}
      </div>
    </Modal>
  )
}

/**
 * Which roles can see a private channel or category.
 *
 * Admins are deliberately absent: ADMINISTRATOR implies every permission
 * everywhere, so listing them as a choice would suggest they could be
 * excluded.
 */
export function RolePicker({
  roles,
  value,
  onChange,
}: {
  roles: Role[]
  value: string[]
  onChange: (next: string[]) => void
}) {
  const toggle = (id: string) =>
    onChange(value.includes(id) ? value.filter((entry) => entry !== id) : [...value, id])

  return (
    <div>
      <span className="label flex items-center gap-1.5">
        <Users size={12} /> Roles with access
      </span>
      <div className="flex flex-wrap gap-1.5">
        {roles.map((role) => {
          const on = value.includes(role.id)
          const colour = role.color ?? 'var(--text-muted)'
          return (
            <button
              key={role.id}
              onClick={() => toggle(role.id)}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs font-medium transition-colors"
              style={{
                borderColor: on ? colour : 'var(--border)',
                background: on ? `color-mix(in oklab, ${colour} 18%, transparent)` : 'transparent',
                color: on ? 'var(--text)' : 'var(--text-muted)',
              }}
            >
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: colour, opacity: on ? 1 : 0.4 }}
              />
              {role.name}
              {role.is_default && (
                <span style={{ color: 'var(--text-faint)' }}>· everyone</span>
              )}
            </button>
          )
        })}
      </div>
      {value.length === 0 && (
        <p className="text-xs mt-2" style={{ color: 'var(--warning)' }}>
          With no roles picked, only admins will see it.
        </p>
      )}
    </div>
  )
}
