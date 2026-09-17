import { Plus, Save, Shield, Trash2, Upload, X } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { P, toBits } from '../../lib/perms'
import { useStore } from '../../lib/store'
import type { PermissionDef, Role } from '../../lib/types'
import { Field, Switch, toast, useConfirm } from '../ui'

const GROUP_LABELS: Record<string, string> = {
  general: 'General',
  text: 'Text channels',
  voice: 'Voice channels',
  admin: 'Moderation & administration',
}

export default function Roles() {
  const roles = useStore((s) => s.roles)
  const members = useStore((s) => s.members)
  const myPermissions = useStore((s) => s.permissions)
  const confirm = useConfirm()

  const [catalog, setCatalog] = useState<PermissionDef[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<Role | null>(null)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    api.permissionCatalog().then(setCatalog).catch(() => undefined)
  }, [])

  useEffect(() => {
    if (!selectedId && roles.length) setSelectedId(roles[0].id)
  }, [roles, selectedId])

  useEffect(() => {
    const role = roles.find((r) => r.id === selectedId)
    setDraft(role ? { ...role } : null)
  }, [selectedId, roles])

  const grouped = useMemo(() => {
    const map = new Map<string, PermissionDef[]>()
    for (const permission of catalog) {
      const list = map.get(permission.group) ?? []
      list.push(permission)
      map.set(permission.group, list)
    }
    return map
  }, [catalog])

  const memberCount = (roleId: string) =>
    Object.values(members).filter((member) => member.roles.includes(roleId)).length

  const create = async () => {
    try {
      const role = await api.createRole({ name: 'New role', permissions: Number(P.VIEW_CHANNELS) })
      setSelectedId(role.id)
      toast.success('Role created.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not create the role.')
    }
  }

  const save = async () => {
    if (!draft) return
    setSaving(true)
    try {
      await api.updateRole(draft.id, {
        name: draft.name,
        color: draft.color ?? '',
        permissions: draft.permissions,
        hoist: draft.hoist,
        mentionable: draft.mentionable,
        position: draft.position,
        icon_url: draft.icon_url ?? '',
        badge: draft.badge,
      })
      toast.success('Role saved.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save the role.')
    } finally {
      setSaving(false)
    }
  }

  const remove = async () => {
    if (!draft) return
    const ok = await confirm({
      title: `Delete the ${draft.name} role?`,
      body: `${memberCount(draft.id)} member(s) will lose it. This cannot be undone.`,
      confirmLabel: 'Delete role',
      danger: true,
    })
    if (!ok) return
    try {
      await api.deleteRole(draft.id)
      setSelectedId(null)
      toast.success('Role deleted.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not delete the role.')
    }
  }

  const bits = draft ? BigInt(draft.permissions) : 0n
  const isAdminRole = (bits & P.ADMINISTRATOR) !== 0n

  return (
    <div className="flex flex-col lg:flex-row gap-5 animate-fade-in">
      <aside className="lg:w-56 shrink-0">
        <div className="flex items-center justify-between mb-2">
          <span className="label !mb-0">Roles</span>
          <button className="btn btn-ghost !p-1.5" onClick={create} title="Create role">
            <Plus size={15} />
          </button>
        </div>
        <div className="space-y-1">
          {roles.map((role) => (
            <button
              key={role.id}
              onClick={() => setSelectedId(role.id)}
              className="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm transition-colors text-left"
              style={{
                background: selectedId === role.id ? 'var(--surface-2)' : 'transparent',
                color: selectedId === role.id ? 'var(--text)' : 'var(--text-muted)',
              }}
            >
              <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ background: role.color ?? 'var(--text-faint)' }} />
              <span className="flex-1 min-w-0 truncate font-medium">{role.name}</span>
              {role.badge && (
                <span
                  className="text-[0.6rem] font-bold px-1 rounded uppercase shrink-0"
                  style={{
                    background: role.color
                      ? `color-mix(in oklab, ${role.color} 22%, transparent)`
                      : 'var(--surface-3)',
                    color: role.color ?? 'var(--text-muted)',
                  }}
                >
                  {role.badge}
                </span>
              )}
              <span className="text-[0.68rem] tabular-nums" style={{ color: 'var(--text-faint)' }}>
                {memberCount(role.id)}
              </span>
            </button>
          ))}
        </div>
      </aside>

      <div className="flex-1 min-w-0">
        {!draft ? (
          <p className="text-sm py-8 text-center" style={{ color: 'var(--text-faint)' }}>
            Select a role to edit it.
          </p>
        ) : (
          <div className="space-y-5">
            <section className="card p-5 space-y-4">
              <div className="flex items-start justify-between gap-3">
                <h3 className="font-semibold">
                  {draft.name}
                  {draft.is_default && (
                    <span className="text-xs font-normal ml-2" style={{ color: 'var(--text-faint)' }}>
                      default role — every member has it
                    </span>
                  )}
                </h3>
                {!draft.is_default && (
                  <button className="btn btn-danger !py-1.5 !px-2.5" onClick={remove}>
                    <Trash2 size={14} />
                  </button>
                )}
              </div>

              <div className="grid sm:grid-cols-2 gap-4">
                <Field label="Name">
                  <input className="input" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
                </Field>
                <Field label="Colour" hint="Colours the member's name in chat.">
                  <div className="flex gap-2">
                    <label className="w-9 h-9 rounded-lg border shrink-0 cursor-pointer" style={{ background: draft.color ?? 'var(--surface-3)' }}>
                      <input
                        type="color"
                        className="opacity-0 w-full h-full cursor-pointer"
                        value={draft.color ?? '#8b93a7'}
                        onChange={(e) => setDraft({ ...draft, color: e.target.value })}
                      />
                    </label>
                    <input
                      className="input font-mono"
                      value={draft.color ?? ''}
                      placeholder="No colour"
                      onChange={(e) => setDraft({ ...draft, color: e.target.value || null })}
                    />
                  </div>
                </Field>
              </div>

              <div className="grid sm:grid-cols-2 gap-3">
                <Switch
                  checked={draft.hoist}
                  onChange={(v) => setDraft({ ...draft, hoist: v })}
                  label="Show separately in the member list"
                />
                <Switch
                  checked={draft.mentionable}
                  onChange={(v) => setDraft({ ...draft, mentionable: v })}
                  label="Anyone can @mention this role"
                />
              </div>

              <div className="grid sm:grid-cols-2 gap-4">
                <Field label="Badge" hint="Up to 8 characters, shown beside the name in chat.">
                  <input
                    className="input"
                    value={draft.badge}
                    maxLength={8}
                    onChange={(e) => setDraft({ ...draft, badge: e.target.value })}
                    placeholder="MOD"
                  />
                </Field>
                <Field label="Icon" hint="A small image shown beside the name.">
                  <div className="flex gap-2 items-center">
                    {draft.icon_url && (
                      <img
                        src={draft.icon_url}
                        alt=""
                        className="w-9 h-9 object-contain rounded-lg shrink-0"
                        style={{ background: 'var(--surface-2)' }}
                      />
                    )}
                    <label className="btn btn-subtle cursor-pointer shrink-0">
                      <Upload size={14} /> {draft.icon_url ? 'Replace' : 'Upload'}
                      <input
                        type="file"
                        accept="image/png,image/webp,image/gif,image/jpeg"
                        className="hidden"
                        onChange={async (event) => {
                          const file = event.target.files?.[0]
                          if (!file) return
                          try {
                            const attachment = await api.upload(file)
                            setDraft({ ...draft, icon_url: attachment.url })
                          } catch (error) {
                            toast.error(
                              error instanceof ApiError ? error.message : 'Upload failed.',
                            )
                          }
                        }}
                      />
                    </label>
                    {draft.icon_url && (
                      <button
                        className="btn btn-ghost !p-2 shrink-0"
                        onClick={() => setDraft({ ...draft, icon_url: null })}
                        aria-label="Remove icon"
                      >
                        <X size={14} />
                      </button>
                    )}
                  </div>
                </Field>
              </div>

              <Field label="Rank" hint="Higher ranks can manage lower ones. Your own rank caps what you can set.">
                <input
                  className="input"
                  type="number"
                  value={draft.position}
                  disabled={draft.is_default}
                  onChange={(e) => setDraft({ ...draft, position: Number(e.target.value) })}
                />
              </Field>
            </section>

            <section className="card p-5">
              <h3 className="font-semibold mb-1">Permissions</h3>
              <p className="text-xs mb-4" style={{ color: 'var(--text-muted)' }}>
                You can only grant permissions you hold yourself.
              </p>

              {isAdminRole && (
                <div
                  className="flex items-start gap-2 p-3 rounded-lg mb-4 text-xs leading-relaxed"
                  style={{ background: 'color-mix(in oklab, var(--warning) 12%, transparent)', color: 'var(--warning)' }}
                >
                  <Shield size={14} className="shrink-0 mt-0.5" />
                  <span>
                    <strong>Administrator</strong> grants every permission, including ones added in future
                    versions, and bypasses channel overrides.
                  </span>
                </div>
              )}

              <div className="space-y-5">
                {['general', 'text', 'voice', 'admin'].map((group) => {
                  const permissions = grouped.get(group) ?? []
                  if (!permissions.length) return null
                  return (
                    <div key={group}>
                      <p className="label">{GROUP_LABELS[group]}</p>
                      <div className="grid sm:grid-cols-2 gap-2.5">
                        {permissions.map((permission) => {
                          const bit = toBits(permission.bit)
                          const on = (bits & bit) === bit
                          const iHaveIt = (myPermissions & P.ADMINISTRATOR) !== 0n || (myPermissions & bit) === bit
                          return (
                            <Switch
                              key={permission.key}
                              checked={on}
                              disabled={!iHaveIt || (isAdminRole && bit !== P.ADMINISTRATOR)}
                              label={permission.label}
                              onChange={(value) =>
                                setDraft({
                                  ...draft,
                                  permissions: Number(value ? bits | bit : bits & ~bit),
                                })
                              }
                            />
                          )
                        })}
                      </div>
                    </div>
                  )
                })}
              </div>
            </section>

            <div className="flex justify-end sticky bottom-0 py-3" style={{ background: 'var(--bg)' }}>
              <button className="btn btn-primary" onClick={save} disabled={saving}>
                <Save size={15} /> {saving ? 'Saving…' : 'Save role'}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
