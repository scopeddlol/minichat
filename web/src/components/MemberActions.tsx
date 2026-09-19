import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import { useInbox } from '../lib/direct'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import { useContextMenu, type MenuItem } from './ContextMenu'
import { copyText, Modal, toast, useConfirm } from './ui'

/** Member actions share the same menu provider as channels and messages. */
export default function MemberActions({ onProfile }: { onProfile: (id: string) => void }) {
  const menu = useContextMenu()
  const confirm = useConfirm()
  const [selected, setSelected] = useState<string | null>(null)
  const [nickname, setNickname] = useState('')
  const [busy, setBusy] = useState(false)
  const { members, roles, me, permissions } = useStore()
  const member = selected ? members[selected] : null
  const topRank = Math.max(0, ...roles.filter(r => me?.roles.includes(r.id)).map(r => r.position))
  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true)
    try { await fn(); toast.success('Member updated.') }
    catch (e) { toast.error(e instanceof Error ? e.message : 'Could not update member.') }
    finally { setBusy(false) }
  }
  useEffect(() => {
    const open = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent && !(event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10'))) return
      const target = (event.target as HTMLElement).closest<HTMLElement>('[data-user-id]')
      if (!target || (event.target as HTMLElement).closest('input,textarea,a')) return
      const state = useStore.getState()
      const user = state.members[target.dataset.userId ?? '']
      if (!user) return
      const rank = (ids: string[]) => Math.max(0, ...state.roles.filter(r => ids.includes(r.id)).map(r => r.position))
      const manageable = !user.is_operator && user.id !== state.me?.id && (state.me?.is_operator || rank(state.me?.roles ?? []) > rank(user.roles))
      const items: MenuItem[] = [{ label: 'View profile', onSelect: () => onProfile(user.id) }]
      if (user.id !== state.me?.id && state.relationships[user.id]?.kind !== 'blocked') items.push({ label: 'Message', onSelect: () => { void useInbox.getState().openPeer(user.id).catch(e => toast.error(e.message)) } })
      if (manageable && (can(state.permissions, P.MANAGE_ROLES) || can(state.permissions, P.MANAGE_NICKNAMES))) items.push({ label: 'Roles & nickname', onSelect: () => { setSelected(user.id); setNickname(user.display_name) } })
      items.push({ label: 'Copy member ID', onSelect: () => { void copyText(user.id, 'Member ID copied') } })
      const moderate = async (label: string, action: () => Promise<unknown>) => {
        if (!await confirm({ title: `${label} ${user.display_name}?`, body: 'This changes their access to the community.', confirmLabel: label, danger: true })) return
        try { await action(); toast.success(`${label} completed.`) }
        catch (e) { toast.error(e instanceof Error ? e.message : 'Could not update member.') }
      }
      if (manageable && can(state.permissions, P.MOVE_MEMBERS)) items.push({ label: 'Disconnect from voice', onSelect: () => { void moderate('Disconnect', () => api.forceDisconnect(user.id)) } })
      if (manageable && can(state.permissions, P.KICK_MEMBERS)) items.push({ label: 'Kick member', danger: true, onSelect: () => { void moderate('Kick', () => api.kickMember(user.id)) } })
      if (manageable && can(state.permissions, P.BAN_MEMBERS)) items.push({ label: 'Ban member', danger: true, onSelect: () => { void moderate('Ban', () => api.banMember(user.id, '')) } })
      event.preventDefault(); event.stopPropagation()
      const rect = target.getBoundingClientRect()
      menu.open({ clientX: event instanceof MouseEvent ? event.clientX : rect.left, clientY: event instanceof MouseEvent ? event.clientY : rect.bottom, preventDefault: () => undefined }, items)
    }
    document.addEventListener('contextmenu', open, true)
    document.addEventListener('keydown', open, true)
    return () => { document.removeEventListener('contextmenu', open, true); document.removeEventListener('keydown', open, true) }
  }, [menu, onProfile, confirm])
  return <Modal open={Boolean(member)} onClose={() => setSelected(null)} title={`Manage ${member?.display_name ?? 'member'}`}>
    {member && <div className="p-6 space-y-6">
      {can(permissions, P.MANAGE_NICKNAMES) && <form onSubmit={e => { e.preventDefault(); void act(() => api.updateMember(member.id, { display_name: nickname })) }}>
        <label className="label" htmlFor="member-nickname">Community nickname</label>
        <div className="flex gap-2"><input id="member-nickname" className="input min-w-0" value={nickname} maxLength={48} onChange={e => setNickname(e.target.value)}/><button className="btn btn-subtle" disabled={busy || !nickname.trim()}>Save</button></div>
      </form>}
      {can(permissions, P.MANAGE_ROLES) && <div><h3 className="label">Roles</h3>{roles.filter(r => !r.is_default && (me?.is_operator || r.position < topRank)).map(role => <label className="flex items-center gap-3 py-2" key={role.id}><input type="checkbox" disabled={busy} checked={member.roles.includes(role.id)} onChange={e => { const enabled = e.target.checked; void act(() => enabled ? api.addMemberRole(member.id, role.id) : api.removeMemberRole(member.id, role.id)) }}/><span>{role.name}</span></label>)}</div>}
    </div>}
  </Modal>
}
