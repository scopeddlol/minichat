import { useEffect, useRef, useState } from 'react'
import { ArrowLeft, MessageSquare, Phone, Send, X } from 'lucide-react'
import { direct, useInbox, type DirectMessage } from '../lib/direct'
import { gateway } from '../lib/gateway'
import { useStore } from '../lib/store'
import { Avatar, EmptyState, toast } from './ui'
import { formatTimestamp } from '../lib/format'

export default function DirectMessages() {
  const { conversations, active, refresh } = useInbox()
  const members = useStore(s => s.members)
  const me = useStore(s => s.me)
  const voiceEnabled = useStore(s => s.voiceEnabled)
  const [query, setQuery] = useState('')
  const [messages, setMessages] = useState<DirectMessage[]>([])
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [loading, setLoading] = useState(false)
  const [more, setMore] = useState(false)
  const [error, setError] = useState('')
  const list = useRef<HTMLDivElement>(null)
  const conversation = conversations.find(c => c.id === active)
  const peer = conversation && members[conversation.peer_id]
  useEffect(() => { void refresh().catch(e => setError(e.message)) }, [refresh])
  useEffect(() => {
    if (!active) return
    let cancelled = false
    setMessages([]); setLoading(true); setError('')
    const load = () => direct.history(active).then(rows => {
      if (cancelled) return
      setMessages(existing => [...new Map([...rows, ...existing].map(m => [m.id, m])).values()].sort((a,b) => a.id.localeCompare(b.id)))
      setMore(rows.length === 50)
      setLoading(false)
      requestAnimationFrame(() => { if (list.current) list.current.scrollTop = list.current.scrollHeight })
    }).catch(e => { if (!cancelled) { setError(e.message); setLoading(false) } })
    void load()
    const off = gateway.on(event => {
      if (event.t === 'READY') void load()
      if (event.t !== 'DIRECT_MESSAGE' || event.d.conversation_id !== active) return
      const nearBottom = !list.current || list.current.scrollHeight - list.current.scrollTop - list.current.clientHeight < 120
      setMessages(rows => [...new Map([...rows, event.d as DirectMessage].map(m => [m.id,m])).values()].sort((a,b) => a.id.localeCompare(b.id)))
      if (nearBottom) requestAnimationFrame(() => { if (list.current) list.current.scrollTop = list.current.scrollHeight })
    })
    return () => { cancelled = true; off() }
  }, [active])
  useEffect(() => {
    const last = messages.at(-1)
    if (!active || !last) return
    const ack = () => { if (document.visibilityState === 'visible') void direct.ack(active, last.id).catch(() => undefined) }
    ack(); document.addEventListener('visibilitychange', ack)
    return () => document.removeEventListener('visibilitychange', ack)
  }, [active, messages])
  const send = async () => {
    if (!active || busy || !drafts[active]?.trim()) return
    const id = active; const content = drafts[id]
    setBusy(true)
    try {
      const message = await direct.send(id, content)
      if (useInbox.getState().active === id) setMessages(rows => [...new Map([...rows,message].map(m => [m.id,m])).values()].sort((a,b) => a.id.localeCompare(b.id)))
      setDrafts(d => ({ ...d, [id]: '' }))
      requestAnimationFrame(() => { if (list.current) list.current.scrollTop = list.current.scrollHeight })
    } catch (e) { toast.error(e instanceof Error ? e.message : 'Message failed. Your draft was kept.') }
    finally { setBusy(false) }
  }
  return <section className="inbox" aria-label="Direct messages">
    <aside className={`inbox-list ${active ? 'inbox-list-selected' : ''}`}>
      <header className="workspace-header"><h1>Messages</h1><button className="btn btn-ghost !p-2" aria-label="Back to community" onClick={() => useInbox.setState({ open: false })}><X size={18}/></button></header>
      <div className="p-4"><input className="input" aria-label="Find someone to message" placeholder="Find someone to message…" value={query} onChange={e => setQuery(e.target.value)}/></div>
      <div className="flex-1 min-h-0 overflow-y-auto scroll-thin px-3 pb-4">
        {query ? Object.values(members).filter(m => m.id !== me?.id && !m.is_suspended && `${m.display_name} ${m.username}`.toLowerCase().includes(query.toLowerCase())).map(m => <button key={m.id} className="conversation-row" onClick={() => { void useInbox.getState().openPeer(m.id).then(() => setQuery('')).catch(e => toast.error(e.message)) }}><Avatar id={m.id} name={m.display_name} src={m.avatar_url} size="md"/><span className="truncate">{m.display_name}</span></button>) : conversations.map(c => <button key={c.id} className={`conversation-row ${active === c.id ? 'selected' : ''}`} onClick={() => useInbox.setState({ active: c.id })}>
          <Avatar id={c.peer_id} name={members[c.peer_id]?.display_name ?? 'Member'} src={members[c.peer_id]?.avatar_url} presence={members[c.peer_id]?.presence}/>
          <span className="min-w-0 flex-1"><strong className="block truncate">{members[c.peer_id]?.display_name ?? 'Member'}</strong><span className="block truncate text-xs text-[var(--text-muted)]">{c.last_content ?? 'Say hello'}</span></span>{c.unread > 0 && <span className="unread-dot" aria-label={`${c.unread} unread messages`}>{c.unread}</span>}
        </button>)}
        {!query && !conversations.length && <p className="p-3 text-sm text-[var(--text-muted)]">A little space for one-to-one conversations. Find a member above to get started.</p>}
      </div>
    </aside>
    <div className={`inbox-thread ${!active ? 'inbox-thread-empty' : ''}`}>
      {active ? <>
        <header className="workspace-header"><button className="btn btn-ghost !p-2" aria-label="Back to messages" onClick={() => useInbox.setState({ active: null })}><ArrowLeft size={18}/></button><div className="min-w-0 flex-1"><h2 className="truncate">{peer?.display_name ?? 'Conversation'}</h2><p className="text-xs text-[var(--text-muted)]">Just the two of you</p></div><button className="btn btn-subtle" disabled={!voiceEnabled} title={voiceEnabled ? 'Start a call' : 'Calls are not configured'} onClick={() => void direct.call(active).catch(e => toast.error(e.message))}><Phone size={16}/><span className="hidden sm:inline">Call</span></button></header>
        <div ref={list} className="direct-history scroll-thin">
          {more && <button className="btn btn-subtle mx-auto mb-6" disabled={loading} onClick={async () => { setLoading(true); try { const rows = await direct.history(active, messages[0]?.id); setMessages(old => [...new Map([...rows,...old].map(m => [m.id,m])).values()].sort((a,b) => a.id.localeCompare(b.id))); setMore(rows.length === 50) } catch(e) { toast.error(String(e)) } finally { setLoading(false) } }}>Load earlier messages</button>}
          {loading && <p role="status">Loading messages…</p>}
          {!loading && !messages.length && <EmptyState icon={<MessageSquare size={24}/>} title={`Say hello${peer ? ` to ${peer.display_name}` : ''}`} body="This is the beginning of your conversation."/>}
          {messages.map(m => <article className="direct-message" key={m.id} data-user-id={m.author_id}><Avatar id={m.author_id} name={members[m.author_id]?.display_name ?? 'Member'} src={members[m.author_id]?.avatar_url}/><div className="min-w-0 flex-1"><div className="flex flex-wrap items-baseline gap-x-3"><strong>{members[m.author_id]?.display_name ?? 'Member'}</strong><time className="text-xs text-[var(--text-faint)]">{formatTimestamp(m.created_at)}</time></div><p className="whitespace-pre-wrap break-words mt-1">{m.content}</p></div></article>)}
        </div>
        {error && <p role="alert" className="p-4 text-[var(--danger)]">{error}</p>}
        <form className="direct-composer" onSubmit={e => { e.preventDefault(); void send() }}><textarea className="input resize-none" aria-label="Direct message" placeholder={`Message ${peer?.display_name ?? 'member'}…`} maxLength={4000} rows={2} value={drafts[active] ?? ''} onChange={e => setDrafts(d => ({ ...d, [active]: e.target.value }))} onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send() } }}/><button className="btn btn-primary" disabled={busy || !drafts[active]?.trim()} aria-label="Send message"><Send size={18}/></button></form>
      </> : <EmptyState icon={<MessageSquare size={28}/>} title="A conversation, just for you" body="Choose a conversation or find someone to message."/>}
    </div>
  </section>
}
