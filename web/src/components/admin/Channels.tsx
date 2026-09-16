import { ChevronDown, FolderPlus, Plus, Save, Settings2, Trash2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { toBits } from '../../lib/perms'
import { useStore } from '../../lib/store'
import type { Channel, PermissionDef } from '../../lib/types'
import { ChannelIcon } from '../Sidebar'
import { Field, Modal, Switch, toast, useConfirm } from '../ui'

export default function Channels() {
  const channels = useStore((s) => s.channels)
  const categories = useStore((s) => s.categories)
  const confirm = useConfirm()

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [newCategory, setNewCategory] = useState('')

  const selected = channels.find((channel) => channel.id === selectedId) ?? null

  return (
    <div className="flex flex-col lg:flex-row gap-5 animate-fade-in">
      <aside className="lg:w-60 shrink-0 space-y-4">
        <div>
          <div className="flex items-center justify-between mb-2">
            <span className="label !mb-0">Channels</span>
            <button className="btn btn-ghost !p-1.5" onClick={() => setCreating(true)} title="Create channel">
              <Plus size={15} />
            </button>
          </div>
          <div className="space-y-1">
            {channels.map((channel) => (
              <button
                key={channel.id}
                onClick={() => setSelectedId(channel.id)}
                className="w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm transition-colors text-left"
                style={{
                  background: selectedId === channel.id ? 'var(--surface-2)' : 'transparent',
                  color: selectedId === channel.id ? 'var(--text)' : 'var(--text-muted)',
                }}
              >
                <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} size={14} />
                <span className="truncate flex-1">{channel.name}</span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="label">Categories</span>
          <div className="space-y-1">
            {categories.map((category) => (
              <div
                key={category.id}
                className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-sm group"
                style={{ background: 'var(--surface-2)' }}
              >
                <ChevronDown size={13} style={{ color: 'var(--text-faint)' }} />
                <input
                  className="bg-transparent outline-none flex-1 min-w-0 text-sm"
                  defaultValue={category.name}
                  onBlur={async (event) => {
                    const name = event.target.value.trim()
                    if (!name || name === category.name) return
                    try {
                      await api.updateCategory(category.id, name, category.position)
                    } catch {
                      toast.error('Could not rename the category.')
                    }
                  }}
                />
                <button
                  className="btn btn-ghost !p-1 opacity-0 group-hover:opacity-100"
                  onClick={async () => {
                    const ok = await confirm({
                      title: `Delete "${category.name}"?`,
                      body: 'Its channels stay, but become uncategorised.',
                      confirmLabel: 'Delete',
                      danger: true,
                    })
                    if (ok) await api.deleteCategory(category.id).catch(() => toast.error('Failed.'))
                  }}
                  aria-label="Delete category"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
          <div className="flex gap-1.5 mt-2">
            <input
              className="input !py-1.5 !text-sm"
              placeholder="New category"
              value={newCategory}
              onChange={(event) => setNewCategory(event.target.value)}
              onKeyDown={async (event) => {
                if (event.key !== 'Enter' || !newCategory.trim()) return
                try {
                  await api.createCategory(newCategory.trim())
                  setNewCategory('')
                } catch {
                  toast.error('Could not create the category.')
                }
              }}
            />
            <button
              className="btn btn-subtle !px-2 shrink-0"
              disabled={!newCategory.trim()}
              onClick={async () => {
                try {
                  await api.createCategory(newCategory.trim())
                  setNewCategory('')
                } catch {
                  toast.error('Could not create the category.')
                }
              }}
            >
              <FolderPlus size={14} />
            </button>
          </div>
        </div>
      </aside>

      <div className="flex-1 min-w-0">
        {selected ? (
          <ChannelEditor key={selected.id} channel={selected} onDeleted={() => setSelectedId(null)} />
        ) : (
          <p className="text-sm py-8 text-center" style={{ color: 'var(--text-faint)' }}>
            Select a channel to configure it.
          </p>
        )}
      </div>

      <CreateChannelModal open={creating} onClose={() => setCreating(false)} onCreated={setSelectedId} />
    </div>
  )
}

function ChannelEditor({ channel, onDeleted }: { channel: Channel; onDeleted: () => void }) {
  const categories = useStore((s) => s.categories)
  const roles = useStore((s) => s.roles)
  const confirm = useConfirm()

  const [draft, setDraft] = useState(channel)
  const [saving, setSaving] = useState(false)
  const [catalog, setCatalog] = useState<PermissionDef[]>([])
  const [overwrites, setOverwrites] = useState<Record<string, { allow: bigint; deny: bigint }>>({})
  const [permRole, setPermRole] = useState<string>('')

  useEffect(() => setDraft(channel), [channel])

  useEffect(() => {
    api.permissionCatalog().then(setCatalog).catch(() => undefined)
    api
      .overwrites(channel.id)
      .then((list) => {
        const map: Record<string, { allow: bigint; deny: bigint }> = {}
        for (const entry of list) map[entry.role_id] = { allow: toBits(entry.allow), deny: toBits(entry.deny) }
        setOverwrites(map)
      })
      .catch(() => undefined)
  }, [channel.id])

  useEffect(() => {
    if (!permRole && roles.length) setPermRole(roles.find((r) => r.is_default)?.id ?? roles[0].id)
  }, [roles, permRole])

  const relevant = catalog.filter((permission) =>
    channel.kind === 'voice'
      ? permission.group === 'voice' || permission.key === 'VIEW_CHANNELS'
      : permission.group === 'text' || permission.key === 'VIEW_CHANNELS',
  )

  const current = overwrites[permRole] ?? { allow: 0n, deny: 0n }

  const setPermission = async (bit: bigint, state: 'allow' | 'deny' | 'inherit') => {
    const next = {
      allow: state === 'allow' ? current.allow | bit : current.allow & ~bit,
      deny: state === 'deny' ? current.deny | bit : current.deny & ~bit,
    }
    setOverwrites((value) => ({ ...value, [permRole]: next }))
    try {
      await api.setOverwrite(channel.id, permRole, Number(next.allow), Number(next.deny))
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save the override.')
    }
  }

  const save = async () => {
    setSaving(true)
    try {
      await api.updateChannel(channel.id, {
        name: draft.name,
        topic: draft.topic,
        category_id: draft.category_id,
        slowmode: draft.slowmode,
        is_private: draft.is_private,
        user_limit: draft.user_limit,
      })
      toast.success('Channel saved.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save the channel.')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-5">
      <section className="card p-5 space-y-4">
        <div className="flex items-center justify-between gap-3">
          <h3 className="font-semibold flex items-center gap-2">
            <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} /> {channel.name}
          </h3>
          <button
            className="btn btn-danger !py-1.5 !px-2.5"
            onClick={async () => {
              const ok = await confirm({
                title: `Delete #${channel.name}?`,
                body: 'Every message in this channel is deleted permanently.',
                confirmLabel: 'Delete channel',
                danger: true,
              })
              if (!ok) return
              try {
                await api.deleteChannel(channel.id)
                onDeleted()
                toast.success('Channel deleted.')
              } catch {
                toast.error('Could not delete the channel.')
              }
            }}
          >
            <Trash2 size={14} />
          </button>
        </div>

        <div className="grid sm:grid-cols-2 gap-4">
          <Field label="Name">
            <input className="input" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
          </Field>
          <Field label="Category">
            <select
              className="input"
              value={draft.category_id ?? ''}
              onChange={(e) => setDraft({ ...draft, category_id: e.target.value || null })}
            >
              <option value="">Uncategorised</option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </select>
          </Field>
        </div>

        <Field label="Topic" hint="Shown in the channel header.">
          <input className="input" value={draft.topic} onChange={(e) => setDraft({ ...draft, topic: e.target.value })} />
        </Field>

        {channel.kind === 'voice' ? (
          <Field label="User limit" hint="0 means unlimited.">
            <input
              className="input"
              type="number"
              min={0}
              max={99}
              value={draft.user_limit}
              onChange={(e) => setDraft({ ...draft, user_limit: Number(e.target.value) })}
            />
          </Field>
        ) : (
          <Field label="Slow mode (seconds)" hint="0 disables it. Moderators are exempt.">
            <input
              className="input"
              type="number"
              min={0}
              max={21600}
              value={draft.slowmode}
              onChange={(e) => setDraft({ ...draft, slowmode: Number(e.target.value) })}
            />
          </Field>
        )}

        <Switch
          checked={draft.is_private}
          onChange={(v) => setDraft({ ...draft, is_private: v })}
          label="Private channel"
          hint="Marks the channel private. Use the overrides below to control exactly who sees it."
        />

        <div className="flex justify-end">
          <button className="btn btn-primary" onClick={save} disabled={saving}>
            <Save size={15} /> {saving ? 'Saving…' : 'Save channel'}
          </button>
        </div>
      </section>

      <section className="card p-5">
        <h3 className="font-semibold flex items-center gap-2 mb-1">
          <Settings2 size={15} style={{ color: 'var(--accent)' }} /> Permission overrides
        </h3>
        <p className="text-xs mb-4" style={{ color: 'var(--text-muted)' }}>
          Overrides apply on top of a role's instance-wide permissions. Deny always wins.
        </p>

        <Field label="Role">
          <select className="input" value={permRole} onChange={(e) => setPermRole(e.target.value)}>
            {roles.map((role) => (
              <option key={role.id} value={role.id}>
                {role.name}
                {role.is_default ? ' (default)' : ''}
              </option>
            ))}
          </select>
        </Field>

        <div className="mt-4 space-y-1">
          {relevant.map((permission) => {
            const bit = toBits(permission.bit)
            const state = (current.allow & bit) === bit ? 'allow' : (current.deny & bit) === bit ? 'deny' : 'inherit'
            return (
              <div
                key={permission.key}
                className="flex items-center justify-between gap-3 px-3 py-2 rounded-lg"
                style={{ background: 'var(--surface-2)' }}
              >
                <span className="text-sm min-w-0 truncate">{permission.label}</span>
                <div className="flex rounded-lg overflow-hidden border shrink-0">
                  {(
                    [
                      ['deny', '✕', 'var(--danger)'],
                      ['inherit', '/', 'var(--text-faint)'],
                      ['allow', '✓', 'var(--success)'],
                    ] as const
                  ).map(([value, symbol, color]) => (
                    <button
                      key={value}
                      onClick={() => void setPermission(bit, value)}
                      className="px-2.5 py-1 text-xs font-bold transition-colors"
                      style={{
                        background: state === value ? `color-mix(in oklab, ${color} 22%, transparent)` : 'var(--surface-1)',
                        color: state === value ? color : 'var(--text-faint)',
                      }}
                      title={value}
                    >
                      {symbol}
                    </button>
                  ))}
                </div>
              </div>
            )
          })}
        </div>
      </section>
    </div>
  )
}

export function CreateChannelModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  onCreated?: (id: string) => void
}) {
  const categories = useStore((s) => s.categories)
  const setActiveChannel = useStore((s) => s.setActiveChannel)
  const [name, setName] = useState('')
  const [kind, setKind] = useState<'text' | 'voice' | 'announcement'>('text')
  const [categoryId, setCategoryId] = useState('')
  const [isPrivate, setIsPrivate] = useState(false)
  const [busy, setBusy] = useState(false)

  const create = async () => {
    if (!name.trim()) return
    setBusy(true)
    try {
      const channel = await api.createChannel({
        name: name.trim(),
        kind,
        category_id: categoryId || null,
        is_private: isPrivate,
      })
      toast.success(`#${channel.name} created.`)
      onCreated?.(channel.id)
      if (!onCreated && channel.kind !== 'voice') setActiveChannel(channel.id)
      setName('')
      setIsPrivate(false)
      onClose()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not create the channel.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Create a channel"
      description="Channels can be renamed or moved at any time."
      width="sm"
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={create} disabled={busy || !name.trim()}>
            {busy ? 'Creating…' : 'Create channel'}
          </button>
        </>
      }
    >
      <div className="px-5 py-4 space-y-4">
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

        <Field label="Name" hint={kind === 'voice' ? undefined : 'Spaces become dashes.'}>
          <input
            className="input"
            value={name}
            autoFocus
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void create()}
            placeholder={kind === 'voice' ? 'Lounge' : 'new-channel'}
          />
        </Field>

        <Field label="Category">
          <select className="input" value={categoryId} onChange={(e) => setCategoryId(e.target.value)}>
            <option value="">Uncategorised</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.name}
              </option>
            ))}
          </select>
        </Field>

        <Switch
          checked={isPrivate}
          onChange={setIsPrivate}
          label="Private channel"
          hint="Hidden from the default role until you grant access."
        />
      </div>
    </Modal>
  )
}
