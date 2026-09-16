import { Ban, Calendar, Gamepad2, Loader2, MessageSquare, Shield, UserMinus, UserPlus } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import { formatFullDate, formatRelative } from '../lib/format'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import type { Member } from '../lib/types'
import { Avatar, Badge, Modal, Spinner, toast, useConfirm } from './ui'

export default function ProfileModal({
  userId,
  onClose,
  onEditProfile,
}: {
  userId: string | null
  onClose: () => void
  onEditProfile: () => void
}) {
  const cached = useStore((s) => (userId ? s.members[userId] : undefined))
  const roles = useStore((s) => s.roles)
  const me = useStore((s) => s.me)
  const permissions = useStore((s) => s.permissions)
  const confirm = useConfirm()

  const [member, setMember] = useState<Member | null>(cached ?? null)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [roleMenuOpen, setRoleMenuOpen] = useState(false)

  useEffect(() => {
    if (!userId) return
    setMember(cached ?? null)
    setLoading(!cached)
    api
      .user(userId)
      .then(setMember)
      .catch(() => undefined)
      .finally(() => setLoading(false))
    // `cached` intentionally omitted: it updates live and would refetch on every keystroke elsewhere.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userId])

  if (!userId) return null

  const isMe = me?.id === userId
  const memberRoles = roles
    .filter((role) => member?.roles.includes(role.id))
    .sort((a, b) => b.position - a.position)
  const topColor = memberRoles.find((role) => role.color)?.color
  const canManageRoles = can(permissions, P.MANAGE_ROLES) && !isMe
  const canKick = can(permissions, P.KICK_MEMBERS) && !isMe && !member?.is_operator
  const canBan = can(permissions, P.BAN_MEMBERS) && !isMe && !member?.is_operator

  const act = async (label: string, action: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await action()
      toast.success(label)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'That action failed.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal open onClose={onClose} width="sm" bare>
      {loading && !member ? (
        <div className="p-12 flex justify-center" style={{ color: 'var(--text-faint)' }}>
          <Spinner size={22} />
        </div>
      ) : !member ? (
        <div className="p-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
          That member no longer exists.
        </div>
      ) : (
        <div>
          <div
            className="h-24 relative"
            style={{
              background: member.banner_url
                ? `url(${member.banner_url}) center/cover`
                : `linear-gradient(135deg, ${member.accent_color}, color-mix(in oklab, ${member.accent_color} 40%, var(--surface-3)))`,
            }}
          />

          <div className="px-5 pb-5 -mt-10">
            <div className="flex items-end justify-between gap-3 mb-3">
              <div style={{ boxShadow: '0 0 0 5px var(--surface-1)', borderRadius: '50%' }}>
                <Avatar
                  id={member.id}
                  name={member.display_name}
                  src={member.avatar_url}
                  accent={member.accent_color}
                  size="xl"
                  presence={member.presence}
                />
              </div>
              {isMe && (
                <button className="btn btn-subtle" onClick={() => { onClose(); onEditProfile() }}>
                  Edit profile
                </button>
              )}
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              <h2 className="text-lg font-semibold" style={{ color: topColor ?? 'var(--text)' }}>
                {member.display_name}
              </h2>
              {member.is_operator && <Badge color="var(--accent)">Operator</Badge>}
              {member.is_suspended && <Badge color="var(--danger)">Suspended</Badge>}
            </div>
            <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
              @{member.username}
              {member.pronouns && ` · ${member.pronouns}`}
            </p>

            {member.custom_status && (
              <p className="text-sm mt-2 px-3 py-2 rounded-lg" style={{ background: 'var(--surface-2)' }}>
                {member.custom_status}
              </p>
            )}

            {member.bio && (
              <div className="mt-4">
                <p className="label">About</p>
                <p className="text-sm whitespace-pre-wrap leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                  {member.bio}
                </p>
              </div>
            )}

            <div className="grid grid-cols-2 gap-2 mt-4">
              {member.favorite_game && (
                <Stat icon={<Gamepad2 size={14} />} label="Playing" value={member.favorite_game} />
              )}
              <Stat icon={<Calendar size={14} />} label="Joined" value={formatFullDate(member.created_at)} />
              {member.message_count !== undefined && (
                <Stat icon={<MessageSquare size={14} />} label="Messages" value={String(member.message_count)} />
              )}
              {member.presence === 'offline' && (
                <Stat icon={<Shield size={14} />} label="Last seen" value={formatRelative(member.last_seen_at)} />
              )}
            </div>

            {memberRoles.length > 0 && (
              <div className="mt-4">
                <p className="label">Roles</p>
                <div className="flex flex-wrap gap-1.5">
                  {memberRoles.map((role) => (
                    <span
                      key={role.id}
                      className="inline-flex items-center gap-1.5 text-[0.76rem] px-2 py-1 rounded-md border"
                      style={{
                        borderColor: role.color ? `color-mix(in oklab, ${role.color} 40%, transparent)` : 'var(--border)',
                        color: role.color ?? 'var(--text-muted)',
                      }}
                    >
                      <span className="w-2 h-2 rounded-full" style={{ background: role.color ?? 'var(--text-faint)' }} />
                      {role.name}
                      {canManageRoles && !role.is_default && (
                        <button
                          className="opacity-60 hover:opacity-100 ml-0.5"
                          onClick={() =>
                            void act('Role removed.', async () => {
                              await api.removeMemberRole(member.id, role.id)
                              setMember({ ...member, roles: member.roles.filter((r) => r !== role.id) })
                            })
                          }
                          aria-label={`Remove ${role.name}`}
                        >
                          ×
                        </button>
                      )}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {(canManageRoles || canKick || canBan) && (
              <div className="mt-5 pt-4 border-t space-y-2">
                {canManageRoles && (
                  <div className="relative">
                    <button className="btn btn-subtle w-full" onClick={() => setRoleMenuOpen((v) => !v)} disabled={busy}>
                      <UserPlus size={15} /> Assign a role
                    </button>
                    {roleMenuOpen && (
                      <div className="card mt-1.5 p-1 max-h-48 overflow-y-auto scroll-thin animate-pop-in">
                        {roles
                          .filter((role) => !member.roles.includes(role.id))
                          .map((role) => (
                            <button
                              key={role.id}
                              className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-sm transition-colors hover:bg-[var(--surface-2)] text-left"
                              onClick={() =>
                                void act('Role assigned.', async () => {
                                  await api.addMemberRole(member.id, role.id)
                                  setMember({ ...member, roles: [...member.roles, role.id] })
                                  setRoleMenuOpen(false)
                                })
                              }
                            >
                              <span className="w-2 h-2 rounded-full" style={{ background: role.color ?? 'var(--text-faint)' }} />
                              {role.name}
                            </button>
                          ))}
                        {roles.filter((role) => !member.roles.includes(role.id)).length === 0 && (
                          <p className="text-xs px-2.5 py-2" style={{ color: 'var(--text-faint)' }}>
                            They already have every role.
                          </p>
                        )}
                      </div>
                    )}
                  </div>
                )}

                <div className="flex gap-2">
                  {canKick && (
                    <button
                      className="btn btn-danger flex-1"
                      disabled={busy}
                      onClick={async () => {
                        const ok = await confirm({
                          title: `Remove ${member.display_name}?`,
                          body: 'Their account is deleted, but their messages stay in the channels.',
                          confirmLabel: 'Remove',
                          danger: true,
                        })
                        if (ok) {
                          await act('Member removed.', () => api.kickMember(member.id))
                          onClose()
                        }
                      }}
                    >
                      {busy ? <Loader2 size={14} className="animate-spin" /> : <UserMinus size={14} />} Remove
                    </button>
                  )}
                  {canBan && (
                    <button
                      className="btn btn-danger flex-1"
                      disabled={busy}
                      onClick={async () => {
                        const ok = await confirm({
                          title: `Ban ${member.display_name}?`,
                          body: 'They will be signed out immediately and cannot sign back in.',
                          confirmLabel: 'Ban',
                          danger: true,
                        })
                        if (ok) await act('Member banned.', () => api.banMember(member.id, ''))
                      }}
                    >
                      <Ban size={14} /> Ban
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </Modal>
  )
}

function Stat({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="px-2.5 py-2 rounded-lg min-w-0" style={{ background: 'var(--surface-2)' }}>
      <p className="text-[0.66rem] font-semibold uppercase tracking-wide flex items-center gap-1" style={{ color: 'var(--text-faint)' }}>
        {icon} {label}
      </p>
      <p className="text-[0.82rem] truncate mt-0.5">{value}</p>
    </div>
  )
}
