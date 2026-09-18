import { ChevronDown, FolderPlus, Lock, Plus, Save, Settings2, Trash2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { toBits } from '../../lib/perms'
import { useStore } from '../../lib/store'
import CategoryDialog from '../CategoryDialog'
import ChannelDialog from '../ChannelDialog'
import type { Category, Channel, PermissionDef } from '../../lib/types'
import Select from '../Select'
import { ChannelIcon } from '../Sidebar'
import { Field, Switch, toast, useConfirm } from '../ui'

export default function Channels() {
  const channels = useStore((s) => s.channels)
  const categories = useStore((s) => s.categories)
  const confirm = useConfirm()

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [editingCategory, setEditingCategory] = useState<Category | null>(null)
  const [newCategory, setNewCategory] = useState('')

  const selected = channels.find((channel) => channel.id === selectedId) ?? null

  return (
    <div className="flex flex-col lg:flex-row gap-5 animate-fade-in">
      <aside className="lg:w-60 shrink-0 space-y-4">
        <div>
          <div className="flex items-center justify-between mb-2">
            <span className="label !mb-0">Channels</span>
            <button
              className="btn btn-ghost !p-1.5"
              onClick={() => setCreating(true)}
              title="Create channel"
              aria-label="Create channel"
            >
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
                <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} emoji={channel.emoji} size={14} />
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
                      await api.updateCategory(category.id, { name })
                    } catch {
                      toast.error('Could not rename the category.')
                    }
                  }}
                />
                {category.is_private && (
                  <Lock size={11} className="shrink-0" style={{ color: 'var(--text-faint)' }} />
                )}
                <button
                  className="btn btn-ghost !p-1 opacity-0 group-hover:opacity-100"
                  onClick={() => setEditingCategory(category)}
                  aria-label="Edit category"
                >
                  <Settings2 size={13} />
                </button>
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
                  await api.createCategory({ name: newCategory.trim() })
                  setNewCategory('')
                } catch {
                  toast.error('Could not create the category.')
                }
              }}
            />
            <button
              className="btn btn-subtle !px-2 shrink-0"
              aria-label="Create category"
              disabled={!newCategory.trim()}
              onClick={async () => {
                try {
                  await api.createCategory({ name: newCategory.trim() })
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

      <ChannelDialog open={creating} onClose={() => setCreating(false)} />
      <CategoryDialog
        open={!!editingCategory}
        category={editingCategory}
        onClose={() => setEditingCategory(null)}
      />
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
        emoji: draft.emoji,
        description: draft.description,
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
            <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} emoji={channel.emoji} /> {channel.name}
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
            <Select
              value={draft.category_id ?? ''}
              onChange={(next) => setDraft({ ...draft, category_id: next || null })}
              ariaLabel="Category"
              options={[
                { value: '', label: 'Uncategorised' },
                ...categories.map((category) => ({ value: category.id, label: category.name })),
              ]}
            />
          </Field>
        </div>

        <Field label="Topic" hint="Shown in the channel header.">
          <input className="input" value={draft.topic} onChange={(e) => setDraft({ ...draft, topic: e.target.value })} />
        </Field>

        <div className="grid sm:grid-cols-2 gap-4">
          <Field label="Emoji" hint="Replaces the # or speaker icon in the sidebar.">
            <input
              className="input"
              value={draft.emoji}
              maxLength={8}
              onChange={(e) => setDraft({ ...draft, emoji: e.target.value })}
              placeholder="🎮"
            />
          </Field>
          <Field label="Sidebar description" hint="A short line under the channel name.">
            <input
              className="input"
              value={draft.description}
              maxLength={100}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              placeholder="where the plans happen"
            />
          </Field>
        </div>

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
          <Select
            value={permRole}
            onChange={setPermRole}
            ariaLabel="Role"
            options={roles.map((role) => ({
              value: role.id,
              label: role.name,
              hint: role.is_default ? 'default role' : undefined,
              swatch: role.color,
            }))}
          />
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

