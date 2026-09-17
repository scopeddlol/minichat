import { Ban, Search, ShieldCheck, UserMinus, UserX } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { formatFullDate, formatRelative } from '../../lib/format'
import { can, P } from '../../lib/perms'
import { useStore } from '../../lib/store'
import type { BanEntry } from '../../lib/types'
import Select from '../Select'
import { Avatar, Badge, toast, useConfirm } from '../ui'

export default function Members({ onOpenProfile }: { onOpenProfile: (userId: string) => void }) {
  const members = useStore((s) => s.members)
  const roles = useStore((s) => s.roles)
  const me = useStore((s) => s.me)
  const permissions = useStore((s) => s.permissions)
  const confirm = useConfirm()

  const [query, setQuery] = useState('')
  const [roleFilter, setRoleFilter] = useState('')
  const [bans, setBans] = useState<BanEntry[]>([])
  const [showBans, setShowBans] = useState(false)

  const canBan = can(permissions, P.BAN_MEMBERS)
  const canKick = can(permissions, P.KICK_MEMBERS)
  const isOperator = Boolean(me?.is_operator)

  useEffect(() => {
    if (canBan) api.bans().then(setBans).catch(() => undefined)
  }, [canBan])

  const list = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return Object.values(members)
      .filter(
        (member) =>
          (!needle ||
            member.display_name.toLowerCase().includes(needle) ||
            member.username.toLowerCase().includes(needle)) &&
          (!roleFilter || member.roles.includes(roleFilter)),
      )
      .sort((a, b) => a.display_name.localeCompare(b.display_name))
  }, [members, query, roleFilter])

  const act = async (label: string, action: () => Promise<unknown>) => {
    try {
      await action()
      toast.success(label)
      if (canBan) setBans(await api.bans())
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'That action failed.')
    }
  }

  return (
    <div className="space-y-4 animate-fade-in">
      <div className="flex gap-2 flex-wrap">
        <div className="relative flex-1 min-w-48">
          <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-faint)' }} />
          <input
            className="input !pl-9"
            placeholder="Search members"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <Select
          width="auto"
          value={roleFilter}
          onChange={setRoleFilter}
          ariaLabel="Filter by role"
          options={[
            { value: '', label: 'All roles' },
            ...roles.map((role) => ({ value: role.id, label: role.name, swatch: role.color })),
          ]}
        />
        {canBan && (
          <button
            className={`btn ${showBans ? 'btn-primary' : 'btn-subtle'}`}
            onClick={() => setShowBans((value) => !value)}
          >
            <Ban size={14} /> Bans ({bans.length})
          </button>
        )}
      </div>

      {showBans ? (
        <div className="card divide-y">
          {bans.map((ban) => (
            <div key={ban.user_id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium truncate">{ban.display_name}</p>
                <p className="text-xs truncate" style={{ color: 'var(--text-muted)' }}>
                  @{ban.username}
                  {ban.reason && ` — ${ban.reason}`}
                </p>
              </div>
              <span className="text-xs shrink-0" style={{ color: 'var(--text-faint)' }}>
                {formatRelative(ban.created_at)}
              </span>
              <button
                className="btn btn-subtle !py-1.5 !px-2.5 shrink-0"
                onClick={() => void act('Ban lifted.', () => api.unbanMember(ban.user_id))}
              >
                Unban
              </button>
            </div>
          ))}
          {!bans.length && (
            <p className="text-sm px-4 py-8 text-center" style={{ color: 'var(--text-faint)' }}>
              Nobody is banned.
            </p>
          )}
        </div>
      ) : (
        <div className="card divide-y overflow-hidden">
          {list.map((member) => {
            const memberRoles = roles.filter((role) => member.roles.includes(role.id) && !role.is_default)
            const topColor = roles
              .filter((role) => member.roles.includes(role.id) && role.color)
              .sort((a, b) => b.position - a.position)[0]?.color

            return (
              <div key={member.id} className="flex items-center gap-3 px-4 py-2.5 hover:bg-[var(--surface-2)] transition-colors">
                <button onClick={() => onOpenProfile(member.id)} className="shrink-0">
                  <Avatar
                    id={member.id}
                    name={member.display_name}
                    src={member.avatar_url}
                    accent={member.accent_color}
                    size="md"
                    presence={member.presence}
                  />
                </button>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <button
                      className="text-sm font-medium truncate hover:underline"
                      style={{ color: topColor ?? 'var(--text)' }}
                      onClick={() => onOpenProfile(member.id)}
                    >
                      {member.display_name}
                    </button>
                    {member.is_operator && <Badge color="var(--accent)">Operator</Badge>}
                    {member.is_suspended && <Badge color="var(--danger)">Suspended</Badge>}
                  </div>
                  <p className="text-xs truncate" style={{ color: 'var(--text-faint)' }}>
                    @{member.username} · joined {formatFullDate(member.created_at)}
                  </p>
                </div>

                <div className="hidden md:flex gap-1 shrink-0 max-w-48 flex-wrap justify-end">
                  {memberRoles.slice(0, 3).map((role) => (
                    <span
                      key={role.id}
                      className="text-[0.68rem] px-1.5 py-0.5 rounded border"
                      style={{
                        color: role.color ?? 'var(--text-muted)',
                        borderColor: role.color ? `color-mix(in oklab, ${role.color} 40%, transparent)` : 'var(--border)',
                      }}
                    >
                      {role.name}
                    </span>
                  ))}
                  {memberRoles.length > 3 && (
                    <span className="text-[0.68rem]" style={{ color: 'var(--text-faint)' }}>
                      +{memberRoles.length - 3}
                    </span>
                  )}
                </div>

                <div className="flex gap-1 shrink-0">
                  {isOperator && member.id !== me?.id && (
                    <button
                      className="btn btn-ghost !p-1.5"
                      title={member.is_operator ? 'Demote from operator' : 'Promote to operator'}
                      onClick={async () => {
                        const ok = await confirm({
                          title: member.is_operator
                            ? `Remove operator from ${member.display_name}?`
                            : `Make ${member.display_name} an operator?`,
                          body: member.is_operator
                            ? 'They keep their roles but lose the automatic full-access flag.'
                            : 'Operators always have every permission and can never be locked out.',
                          confirmLabel: member.is_operator ? 'Demote' : 'Promote',
                          danger: !member.is_operator,
                        })
                        if (ok)
                          await act(
                            member.is_operator ? 'Operator removed.' : 'Operator added.',
                            () => api.updateMember(member.id, { is_operator: !member.is_operator }),
                          )
                      }}
                      style={{ color: member.is_operator ? 'var(--accent)' : undefined }}
                    >
                      <ShieldCheck size={15} />
                    </button>
                  )}

                  {canKick && !member.is_operator && member.id !== me?.id && (
                    <button
                      className="btn btn-ghost !p-1.5"
                      title={member.is_suspended ? 'Lift suspension' : 'Suspend account'}
                      onClick={() =>
                        void act(
                          member.is_suspended ? 'Suspension lifted.' : 'Member suspended.',
                          () => api.updateMember(member.id, { is_suspended: !member.is_suspended }),
                        )
                      }
                      style={{ color: member.is_suspended ? 'var(--warning)' : undefined }}
                    >
                      <UserMinus size={15} />
                    </button>
                  )}

                  {canBan && !member.is_operator && member.id !== me?.id && (
                    <button
                      className="btn btn-ghost !p-1.5"
                      title="Ban member"
                      onClick={async () => {
                        const ok = await confirm({
                          title: `Ban ${member.display_name}?`,
                          body: 'They are signed out immediately and cannot sign back in.',
                          confirmLabel: 'Ban',
                          danger: true,
                        })
                        if (ok) await act('Member banned.', () => api.banMember(member.id, ''))
                      }}
                      style={{ color: 'var(--danger)' }}
                    >
                      <UserX size={15} />
                    </button>
                  )}
                </div>
              </div>
            )
          })}

          {!list.length && (
            <p className="text-sm px-4 py-10 text-center" style={{ color: 'var(--text-faint)' }}>
              No members match that search.
            </p>
          )}
        </div>
      )}
    </div>
  )
}
