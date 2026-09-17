import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { api } from '../lib/api'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import { useInbox } from '../lib/direct'
import { Modal, toast } from './ui'
import { ChannelEditor } from './admin/Channels'

type Item = { label: string; run: () => void | Promise<unknown> }
type Menu = { x: number; y: number; title: string; items: Item[]; trigger: HTMLElement }
export default function ContextMenu({ onProfile, onCreate }: { onProfile: (id: string) => void; onCreate: () => void }) {
  const [menu, setMenu] = useState<Menu | null>(null)
  const [editChannel, setEditChannel] = useState<string | null>(null)
  const [editMember, setEditMember] = useState<string | null>(null)
  const [nickname, setNickname] = useState('')
  const [busy, setBusy] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const { members, roles, channels, me, permissions } = useStore()
  const member = editMember ? members[editMember] : null
  const channel = channels.find(c => c.id === editChannel)
  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true)
    try { await fn() } catch(e) { toast.error(e instanceof Error ? e.message : 'Action failed') } finally { setBusy(false) }
  }
  useEffect(() => {
    const open = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent && !(event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10'))) return
      const target = (event.target as HTMLElement).closest<HTMLElement>('[data-user-id], [data-channel-id], [data-channel-list]')
      if (!target || (event.target as HTMLElement).closest('input,textarea,[contenteditable=true]')) return
      const state = useStore.getState()
      const items: Item[] = []
      let title = 'Channels'
      const userId = target.dataset.userId
      const channelId = target.dataset.channelId
      if (userId && state.members[userId]) {
        const user = state.members[userId]
        title = user.display_name
        items.push({ label: 'View profile', run: () => onProfile(userId) })
        if (userId !== state.me?.id) items.push({ label: 'Message', run: () => useInbox.getState().openPeer(userId) })
        const rank = (ids: string[]) => Math.max(0, ...state.roles.filter(r => ids.includes(r.id)).map(r => r.position))
        const manageable = !user.is_operator && userId !== state.me?.id && (state.me?.is_operator || rank(state.me?.roles ?? []) > rank(user.roles))
        if (manageable && (can(state.permissions, P.MANAGE_ROLES) || can(state.permissions, P.MANAGE_NICKNAMES))) items.push({ label: 'Roles & nickname', run: () => { setEditMember(userId); setNickname(user.display_name) } })
        items.push({ label: 'Copy member ID', run: () => navigator.clipboard.writeText(userId) })
      } else if (channelId) {
        const c = state.channels.find(c => c.id === channelId)
        if (!c) return
        title = c.name
        items.push({ label: 'Open channel', run: () => { useInbox.setState({ open: false }); state.setActiveChannel(channelId) } })
        if (can(state.permissions, P.MANAGE_CHANNELS)) items.push({ label: 'Edit channel & permissions', run: () => setEditChannel(channelId) })
        items.push({ label: 'Copy channel ID', run: () => navigator.clipboard.writeText(channelId) })
      } else if (can(state.permissions, P.MANAGE_CHANNELS)) items.push({ label: 'Create channel', run: onCreate })
      if (!items.length) return
      event.preventDefault()
      const rect = target.getBoundingClientRect()
      setMenu({ x: event instanceof MouseEvent ? event.clientX : rect.left + 12, y: event instanceof MouseEvent ? event.clientY : rect.bottom, title, items, trigger: target })
    }
    document.addEventListener('contextmenu', open)
    document.addEventListener('keydown', open)
    return () => { document.removeEventListener('contextmenu', open); document.removeEventListener('keydown', open) }
  }, [onProfile, onCreate])
  useLayoutEffect(() => {
    if (!menu || !ref.current) return
    const el = ref.current
    el.style.left = `${Math.max(8, Math.min(menu.x, innerWidth - el.offsetWidth - 8))}px`
    el.style.top = `${Math.max(8, Math.min(menu.y, innerHeight - el.offsetHeight - 8))}px`
    el.querySelector('button')?.focus()
  }, [menu])
  useEffect(() => {
    if (!menu) return
    const close = (e: Event) => { if (!ref.current?.contains(e.target as Node)) setMenu(null) }
    const resize = () => setMenu(null)
    document.addEventListener('pointerdown', close)
    document.addEventListener('scroll', close, true)
    window.addEventListener('resize', resize)
    return () => { document.removeEventListener('pointerdown', close); document.removeEventListener('scroll', close, true); window.removeEventListener('resize', resize) }
  }, [menu])
  const topRank = Math.max(0, ...roles.filter(r => me?.roles.includes(r.id)).map(r => r.position))
  return <>
    {menu && createPortal(<div ref={ref} role="menu" aria-label={menu.title} className="context-menu" style={{ left: menu.x, top: menu.y }} onKeyDown={e => {
      const buttons = [...ref.current!.querySelectorAll('button')]
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement)
      if (['ArrowDown','ArrowUp','Home','End'].includes(e.key)) { e.preventDefault(); buttons[e.key === 'Home' ? 0 : e.key === 'End' ? buttons.length-1 : (index + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus() }
      if (e.key === 'Escape' || e.key === 'Tab') { e.preventDefault(); e.stopPropagation(); menu.trigger.focus(); setMenu(null) }
    }}><p className="truncate px-3 py-2 text-xs text-[var(--text-muted)]">{menu.title}</p>{menu.items.map(item => <button role="menuitem" key={item.label} onClick={() => { setMenu(null); void Promise.resolve().then(item.run).catch(e => toast.error(e.message)) }}>{item.label}</button>)}</div>, document.body)}
    <Modal open={Boolean(channel)} onClose={() => setEditChannel(null)} title="Channel settings" width="lg">{channel && <div className="p-6"><ChannelEditor key={channel.id} channel={channel} onDeleted={() => setEditChannel(null)}/></div>}</Modal>
    <Modal open={Boolean(member)} onClose={() => setEditMember(null)} title={`Manage ${member?.display_name ?? 'member'}`}>
      {member && <div className="p-6 space-y-6">
        {can(permissions,P.MANAGE_NICKNAMES) && <form onSubmit={e => { e.preventDefault(); void act(() => api.updateMember(member.id,{ display_name: nickname })).then(() => undefined) }}><label className="label" htmlFor="member-nickname">Community nickname</label><div className="flex gap-2"><input id="member-nickname" className="input min-w-0" value={nickname} maxLength={48} onChange={e => setNickname(e.target.value)}/><button className="btn btn-subtle" disabled={busy || !nickname.trim()}>Save</button></div></form>}
        {can(permissions,P.MANAGE_ROLES) && <div><h3 className="label">Roles</h3><div className="space-y-2">{roles.filter(r => !r.is_default && (me?.is_operator || r.position < topRank)).map(role => <label className="flex items-center gap-3 py-2" key={role.id}><input type="checkbox" disabled={busy} checked={member.roles.includes(role.id)} onChange={e => void act(() => e.target.checked ? api.addMemberRole(member.id,role.id) : api.removeMemberRole(member.id,role.id))}/><span>{role.name}</span></label>)}</div></div>}
      </div>}
    </Modal>
  </>
}
