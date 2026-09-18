import { Search } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useStore } from '../lib/store'
import type { Member, Role } from '../lib/types'
import { Avatar, RoleFlair } from './ui'

/** Members grouped by their highest hoisted role, Discord-style. */
export default function MemberList({ onOpenProfile }: { onOpenProfile: (userId: string) => void }) {
  const members = useStore((s) => s.members)
  const roles = useStore((s) => s.roles)
  const relationships = useStore((s) => s.relationships)
  const [query, setQuery] = useState('')

  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const list = Object.values(members).filter(
      (member) =>
        !needle ||
        member.display_name.toLowerCase().includes(needle) ||
        member.username.toLowerCase().includes(needle),
    )

    // Favourites and friends rise to the top, ahead of hoisted roles: a
    // hoisted role says something about the instance, these say something
    // about you, and that is the more useful sort for finding someone.
    const starred: Member[] = []
    const friends: Member[] = []

    const hoisted = roles.filter((role) => role.hoist).sort((a, b) => b.position - a.position)
    const buckets: { role: Role | null; label: string; members: Member[] }[] = hoisted.map((role) => ({
      role,
      label: role.name,
      members: [],
    }))
    const online: Member[] = []
    const offline: Member[] = []

    for (const member of list) {
      const relation = relationships[member.id]
      if (relation?.favourite) {
        starred.push(member)
        continue
      }
      if (relation?.kind === 'friend') {
        friends.push(member)
        continue
      }
      if (member.presence === 'offline') {
        offline.push(member)
        continue
      }
      const bucket = buckets.find((entry) => entry.role && member.roles.includes(entry.role.id))
      if (bucket) bucket.members.push(member)
      else online.push(member)
    }

    const sort = (a: Member, b: Member) => a.display_name.localeCompare(b.display_name)
    const result: { role: Role | null; label: string; members: Member[] }[] = []

    if (starred.length) result.push({ role: null, label: 'Favourites', members: starred.sort(sort) })
    if (friends.length) result.push({ role: null, label: 'Friends', members: friends.sort(sort) })
    result.push(
      ...buckets
        .filter((bucket) => bucket.members.length)
        .map((bucket) => ({ ...bucket, members: bucket.members.sort(sort) })),
    )

    if (online.length) result.push({ role: null, label: 'Online', members: online.sort(sort) })
    if (offline.length) result.push({ role: null, label: 'Offline', members: offline.sort(sort) })
    return result
  }, [members, roles, relationships, query])

  const onlineCount = Object.values(members).filter((m) => m.presence !== 'offline').length

  return (
    <div className="member-panel h-full min-h-0 flex flex-col" style={{ background: 'var(--surface-0)' }}>
      <header className="px-3 py-2.5 border-b shrink-0 xl:pr-3 pr-12" style={{ borderColor: 'var(--border-soft)' }}>
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
            style={{ color: 'var(--text-faint)' }}
          />
          <input
            className="input !py-1.5 !pl-8 !text-[0.82rem]"
            placeholder={`Search ${Object.keys(members).length} members`}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <p className="text-[0.68rem] mt-1.5 px-0.5" style={{ color: 'var(--text-faint)' }}>
          {onlineCount} online
        </p>
      </header>

      <div className="flex-1 min-h-0 overflow-y-auto scroll-thin px-2 py-2 space-y-3">
        {groups.map((group) => (
          <div key={group.label}>
            <p
              className="text-[0.66rem] font-bold uppercase tracking-wider px-1.5 mb-1"
              style={{ color: group.role?.color ?? 'var(--text-faint)' }}
            >
              {group.label} — {group.members.length}
            </p>
            <div className="space-y-0.5">
              {group.members.map((member) => {
                const roleColor = roles
                  .filter((role) => member.roles.includes(role.id) && role.color)
                  .sort((a, b) => b.position - a.position)[0]?.color

                return (
                  <button
                    key={member.id}
                    data-user-id={member.id}
                    onClick={() => onOpenProfile(member.id)}
                    className="w-full flex items-center gap-2.5 px-1.5 py-1.5 rounded-lg transition-colors hover:bg-[var(--surface-2)] text-left"
                    style={{ opacity: member.presence === 'offline' ? 0.55 : 1 }}
                  >
                    <Avatar
                      id={member.id}
                      name={member.display_name}
                      src={member.avatar_url}
                      frame={member.avatar_frame}
                      accent={member.accent_color}
                      size="sm"
                      presence={member.presence}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-1.5 min-w-0">
                        <span
                          className="text-[0.84rem] font-medium truncate leading-tight"
                          style={{ color: roleColor ?? 'var(--text)' }}
                        >
                          {member.display_name}
                        </span>
                        <RoleFlair roles={roles} memberRoleIds={member.roles} size="xs" />
                      </span>
                      {member.custom_status && (
                        <span className="text-[0.7rem] truncate block" style={{ color: 'var(--text-faint)' }}>
                          {member.custom_status}
                        </span>
                      )}
                    </span>
                  </button>
                )
              })}
            </div>
          </div>
        ))}

        {!groups.length && (
          <p className="text-xs text-center py-6" style={{ color: 'var(--text-faint)' }}>
            No members match that search.
          </p>
        )}
      </div>
    </div>
  )
}
