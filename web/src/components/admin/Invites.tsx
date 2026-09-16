import { Copy, Link2, Plus, Trash2 } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { formatRelative } from '../../lib/format'
import { can, P } from '../../lib/perms'
import { useStore } from '../../lib/store'
import type { Invite } from '../../lib/types'
import { Field, Modal, copyText, toast, useConfirm } from '../ui'

export default function Invites() {
  const roles = useStore((s) => s.roles)
  const members = useStore((s) => s.members)
  const permissions = useStore((s) => s.permissions)
  const confirm = useConfirm()

  const [invites, setInvites] = useState<Invite[]>([])
  const [creating, setCreating] = useState(false)
  const [loading, setLoading] = useState(true)

  const load = () =>
    api
      .invites()
      .then(setInvites)
      .catch(() => undefined)
      .finally(() => setLoading(false))

  useEffect(() => {
    void load()
  }, [])

  return (
    <div className="space-y-4 animate-fade-in">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="font-semibold">Invite links</h3>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            Anyone with a valid link can create an account on this instance.
          </p>
        </div>
        <button className="btn btn-primary shrink-0" onClick={() => setCreating(true)}>
          <Plus size={15} /> New invite
        </button>
      </div>

      <div className="card divide-y overflow-hidden">
        {invites.map((invite) => {
          const exhausted = invite.max_uses > 0 && invite.uses >= invite.max_uses
          const expired = invite.expires_at ? new Date(invite.expires_at.replace(' ', 'T') + 'Z') < new Date() : false
          const dead = invite.revoked || exhausted || expired

          return (
            <div key={invite.code} className="flex items-center gap-3 px-4 py-3" style={{ opacity: dead ? 0.5 : 1 }}>
              <div
                className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
                style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
              >
                <Link2 size={15} />
              </div>

              <div className="min-w-0 flex-1">
                <p className="text-sm font-mono truncate">{invite.code}</p>
                <p className="text-xs truncate" style={{ color: 'var(--text-faint)' }}>
                  {invite.note && `${invite.note} · `}
                  {invite.created_by && members[invite.created_by]
                    ? `by ${members[invite.created_by].display_name} · `
                    : ''}
                  {invite.uses}
                  {invite.max_uses > 0 ? `/${invite.max_uses}` : ''} used
                  {invite.role_id && roles.find((r) => r.id === invite.role_id)
                    ? ` · grants ${roles.find((r) => r.id === invite.role_id)!.name}`
                    : ''}
                </p>
              </div>

              <span className="text-xs shrink-0 hidden sm:block" style={{ color: 'var(--text-faint)' }}>
                {invite.revoked
                  ? 'Revoked'
                  : expired
                    ? 'Expired'
                    : exhausted
                      ? 'Used up'
                      : invite.expires_at
                        ? `expires ${formatRelative(invite.expires_at).replace(' ago', '')}`
                        : 'never expires'}
              </span>

              <button
                className="btn btn-subtle !py-1.5 !px-2.5 shrink-0"
                onClick={() => copyText(invite.url, 'Invite link copied')}
                disabled={dead}
                title="Copy invite link"
              >
                <Copy size={14} />
              </button>

              {!invite.revoked && (
                <button
                  className="btn btn-ghost !p-1.5 shrink-0"
                  style={{ color: 'var(--danger)' }}
                  title="Revoke invite"
                  onClick={async () => {
                    const ok = await confirm({
                      title: 'Revoke this invite?',
                      body: 'The link stops working immediately.',
                      confirmLabel: 'Revoke',
                      danger: true,
                    })
                    if (!ok) return
                    try {
                      await api.revokeInvite(invite.code)
                      void load()
                      toast.success('Invite revoked.')
                    } catch {
                      toast.error('Could not revoke the invite.')
                    }
                  }}
                >
                  <Trash2 size={14} />
                </button>
              )}
            </div>
          )
        })}

        {!invites.length && !loading && (
          <p className="text-sm px-4 py-10 text-center" style={{ color: 'var(--text-faint)' }}>
            No invites yet. Create one to bring people in.
          </p>
        )}
      </div>

      <CreateInviteModal
        open={creating}
        onClose={() => setCreating(false)}
        onCreated={load}
        canAssignRole={can(permissions, P.MANAGE_ROLES)}
      />
    </div>
  )
}

export function CreateInviteModal({
  open,
  onClose,
  onCreated,
  canAssignRole,
}: {
  open: boolean
  onClose: () => void
  onCreated?: () => void
  canAssignRole: boolean
}) {
  const roles = useStore((s) => s.roles)
  const [note, setNote] = useState('')
  const [maxUses, setMaxUses] = useState(0)
  const [expiresIn, setExpiresIn] = useState(168)
  const [roleId, setRoleId] = useState('')
  const [created, setCreated] = useState<Invite | null>(null)
  const [busy, setBusy] = useState(false)

  const create = async () => {
    setBusy(true)
    try {
      const invite = await api.createInvite({
        note,
        max_uses: maxUses,
        expires_in_hours: expiresIn,
        role_id: roleId || null,
      })
      setCreated(invite)
      onCreated?.()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not create the invite.')
    } finally {
      setBusy(false)
    }
  }

  const close = () => {
    setCreated(null)
    setNote('')
    onClose()
  }

  return (
    <Modal
      open={open}
      onClose={close}
      title={created ? 'Invite ready' : 'Create an invite'}
      description={created ? 'Share this link with the person you want to invite.' : undefined}
      width="sm"
      footer={
        created ? (
          <button className="btn btn-primary" onClick={close}>
            Done
          </button>
        ) : (
          <>
            <button className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={create} disabled={busy}>
              {busy ? 'Creating…' : 'Create invite'}
            </button>
          </>
        )
      }
    >
      <div className="px-5 py-4 space-y-4">
        {created ? (
          <>
            <div
              className="flex items-center gap-2 p-3 rounded-xl border"
              style={{ background: 'var(--surface-0)' }}
            >
              <code className="text-xs flex-1 min-w-0 truncate">{created.url}</code>
              <button className="btn btn-primary !py-1.5 !px-2.5 shrink-0" onClick={() => copyText(created.url, 'Invite link copied')}>
                <Copy size={14} />
              </button>
            </div>
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
              {created.max_uses > 0 ? `Valid for ${created.max_uses} use(s)` : 'Unlimited uses'}
              {created.expires_at ? ` · expires ${formatRelative(created.expires_at).replace(' ago', '')}` : ' · never expires'}
            </p>
          </>
        ) : (
          <>
            <Field label="Note" hint="Just for you — helps you remember who a link was for.">
              <input className="input" value={note} onChange={(e) => setNote(e.target.value)} placeholder="For Sam" />
            </Field>
            <Field label="Maximum uses" hint="0 means unlimited.">
              <input
                className="input"
                type="number"
                min={0}
                value={maxUses}
                onChange={(e) => setMaxUses(Number(e.target.value))}
              />
            </Field>
            <Field label="Expires after">
              <select className="input" value={expiresIn} onChange={(e) => setExpiresIn(Number(e.target.value))}>
                <option value={1}>1 hour</option>
                <option value={24}>1 day</option>
                <option value={168}>7 days</option>
                <option value={720}>30 days</option>
                <option value={0}>Never</option>
              </select>
            </Field>
            {canAssignRole && (
              <Field label="Grant a role on join" hint="Optional — useful for pre-approving moderators.">
                <select className="input" value={roleId} onChange={(e) => setRoleId(e.target.value)}>
                  <option value="">No extra role</option>
                  {roles
                    .filter((role) => !role.is_default)
                    .map((role) => (
                      <option key={role.id} value={role.id}>
                        {role.name}
                      </option>
                    ))}
                </select>
              </Field>
            )}
          </>
        )}
      </div>
    </Modal>
  )
}
