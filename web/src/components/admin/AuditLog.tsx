import { ScrollText } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api } from '../../lib/api'
import { formatTimestamp } from '../../lib/format'
import { gateway } from '../../lib/gateway'
import type { AuditEntry } from '../../lib/types'
import { EmptyState, Spinner } from '../ui'

const ACTION_LABELS: Record<string, string> = {
  'instance.setup': 'set up the instance',
  'instance.update': 'updated instance settings',
  'channel.create': 'created a channel',
  'channel.update': 'updated a channel',
  'channel.delete': 'deleted a channel',
  'channel.permissions': 'changed channel permissions',
  'role.create': 'created a role',
  'role.update': 'updated a role',
  'role.delete': 'deleted a role',
  'member.join': 'joined',
  'member.kick': 'removed a member',
  'member.ban': 'banned a member',
  'member.unban': 'lifted a ban',
  'member.suspend': 'suspended a member',
  'member.unsuspend': 'lifted a suspension',
  'member.promote': 'promoted an operator',
  'member.demote': 'demoted an operator',
  'member.rename': 'renamed a member',
  'member.role_add': 'assigned a role',
  'member.role_remove': 'removed a role',
  'message.delete': "deleted someone's message",
  'invite.create': 'created an invite',
  'invite.revoke': 'revoked an invite',
  'webhook.create': 'created a webhook',
  'webhook.delete': 'deleted a webhook',
  'voice.disconnect': 'disconnected someone from voice',
}

const CATEGORIES = ['', 'member', 'channel', 'role', 'invite', 'instance', 'webhook', 'message']

export default function AuditLog() {
  const [entries, setEntries] = useState<AuditEntry[]>([])
  const [filter, setFilter] = useState('')
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)

  useEffect(() => {
    setLoading(true)
    api
      .audit(filter ? { action: filter } : {})
      .then(setEntries)
      .catch(() => undefined)
      .finally(() => setLoading(false))
  }, [filter])

  // New entries stream in over the gateway, scoped to audit-log viewers.
  useEffect(() => {
    const unsubscribe = gateway.on((event) => {
      if (event.t !== 'AUDIT_ENTRY') return
      const entry = event.d as AuditEntry
      if (filter && !entry.action.startsWith(filter)) return
      setEntries((current) => [entry, ...current])
    })
    return () => {
      unsubscribe()
    }
  }, [filter])

  const loadMore = async () => {
    if (!entries.length) return
    setLoadingMore(true)
    try {
      const older = await api.audit({
        before: entries[entries.length - 1].id,
        ...(filter ? { action: filter } : {}),
      })
      setEntries((current) => [...current, ...older])
    } finally {
      setLoadingMore(false)
    }
  }

  return (
    <div className="space-y-4 animate-fade-in max-w-3xl">
      <div className="flex items-center gap-2 flex-wrap">
        {CATEGORIES.map((category) => (
          <button
            key={category || 'all'}
            onClick={() => setFilter(category)}
            className="px-2.5 py-1 rounded-lg text-xs font-medium capitalize transition-colors"
            style={{
              background: filter === category ? 'var(--accent-soft)' : 'var(--surface-2)',
              color: filter === category ? 'var(--accent)' : 'var(--text-muted)',
            }}
          >
            {category || 'Everything'}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="flex justify-center py-12" style={{ color: 'var(--text-faint)' }}>
          <Spinner size={22} />
        </div>
      ) : entries.length ? (
        <>
          <div className="card divide-y overflow-hidden">
            {entries.map((entry) => (
              <div key={entry.id} className="flex items-start gap-3 px-4 py-2.5">
                <span
                  className="w-1.5 h-1.5 rounded-full mt-2 shrink-0"
                  style={{ background: colorFor(entry.action) }}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-sm">
                    <strong>{entry.actor_name ?? 'System'}</strong>{' '}
                    <span style={{ color: 'var(--text-muted)' }}>
                      {ACTION_LABELS[entry.action] ?? entry.action}
                    </span>
                    {entry.detail && <span className="ml-1">— {entry.detail}</span>}
                  </p>
                  <p className="text-[0.7rem] mt-0.5" style={{ color: 'var(--text-faint)' }}>
                    {formatTimestamp(entry.created_at)}
                  </p>
                </div>
              </div>
            ))}
          </div>

          <div className="flex justify-center">
            <button className="btn btn-subtle" onClick={loadMore} disabled={loadingMore}>
              {loadingMore ? 'Loading…' : 'Load older entries'}
            </button>
          </div>
        </>
      ) : (
        <EmptyState icon={<ScrollText size={22} />} title="Nothing logged yet" body="Moderation and configuration changes will appear here." />
      )}
    </div>
  )
}

function colorFor(action: string): string {
  if (action.includes('delete') || action.includes('ban') || action.includes('kick')) return 'var(--danger)'
  if (action.includes('create') || action.includes('join')) return 'var(--success)'
  if (action.includes('update') || action.includes('permissions')) return 'var(--warning)'
  return 'var(--accent)'
}
