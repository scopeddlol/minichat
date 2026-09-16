import {
  Hash, Menu, Pin, Search, Users, WifiOff, X, Download,
} from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import AdminPanel from '../components/AdminPanel'
import Composer from '../components/Composer'
import MemberList from '../components/MemberList'
import MessageList from '../components/MessageList'
import ProfileModal from '../components/ProfileModal'
import SettingsModal from '../components/SettingsModal'
import Sidebar, { ChannelIcon } from '../components/Sidebar'
import VoiceStage from '../components/VoiceStage'
import { CreateChannelModal } from '../components/admin/Channels'
import { CreateInviteModal } from '../components/admin/Invites'
import { Modal, Spinner, EmptyState } from '../components/ui'
import { api } from '../lib/api'
import { formatSlowmode, formatTimestamp } from '../lib/format'
import { gateway } from '../lib/gateway'
import { attachHotkeys, loadBindings } from '../lib/hotkeys'
import { can, P } from '../lib/perms'
import { pruneTyping, useStore } from '../lib/store'
import type { Attachment, Message } from '../lib/types'
import { useVoice } from '../lib/voice'

export default function Chat() {
  const me = useStore((s) => s.me)
  const channels = useStore((s) => s.channels)
  const activeChannelId = useStore((s) => s.activeChannelId)
  const channelPermissions = useStore((s) => s.channelPermissions)
  const permissions = useStore((s) => s.permissions)
  const connection = useStore((s) => s.connection)

  const voiceChannelId = useVoice((s) => s.channelId)
  const voiceConnected = useVoice((s) => s.connected)

  const [sidebarOpen, setSidebarOpen] = useState(false)
  const [membersOpen, setMembersOpen] = useState(() => window.innerWidth >= 1280)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [adminOpen, setAdminOpen] = useState(false)
  const [invitesOpen, setInvitesOpen] = useState(false)
  const [createChannelOpen, setCreateChannelOpen] = useState(false)
  const [profileId, setProfileId] = useState<string | null>(null)
  const [searchOpen, setSearchOpen] = useState(false)
  const [pinsOpen, setPinsOpen] = useState(false)
  const [lightbox, setLightbox] = useState<Attachment | null>(null)
  const [replyTo, setReplyTo] = useState<Message | null>(null)

  const channel = channels.find((c) => c.id === activeChannelId) ?? null
  const perms = channel ? (channelPermissions[channel.id] ?? permissions) : permissions

  // Typing indicators expire on a timer rather than an event.
  useEffect(() => {
    const id = setInterval(pruneTyping, 2000)
    return () => clearInterval(id)
  }, [])

  // Reconnect promptly when the tab comes back or the network returns.
  useEffect(() => {
    const refresh = () => {
      if (document.visibilityState === 'visible') gateway.refresh()
    }
    document.addEventListener('visibilitychange', refresh)
    window.addEventListener('online', refresh)
    window.addEventListener('focus', refresh)
    return () => {
      document.removeEventListener('visibilitychange', refresh)
      window.removeEventListener('online', refresh)
      window.removeEventListener('focus', refresh)
    }
  }, [])

  useEffect(() => setReplyTo(null), [activeChannelId])

  // Voice hotkeys. Bindings are re-read on every press, so changing one in
  // settings takes effect without remounting.
  useEffect(() => {
    return attachHotkeys(loadBindings, {
      onPushToTalk: (active) => useVoice.getState().setPushToTalk(active),
      onToggleMute: () => void useVoice.getState().toggleMute(),
      onToggleDeafen: () => void useVoice.getState().toggleDeafen(),
    })
  }, [])

  // Opening a notification asks the service worker to bring us to the message.
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const data = event.data as { type?: string; channelId?: string; messageId?: string }
      if (data?.type !== 'minichat:navigate' || !data.channelId) return
      if (data.messageId) void useStore.getState().jumpToMessage(data.channelId, data.messageId)
      else useStore.getState().setActiveChannel(data.channelId)
    }
    navigator.serviceWorker?.addEventListener('message', onMessage)
    return () => navigator.serviceWorker?.removeEventListener('message', onMessage)
  }, [])

  // A notification that opened a fresh window carries the channel in the URL.
  useEffect(() => {
    const target = new URLSearchParams(location.search).get('channel')
    if (!target) return
    window.history.replaceState({}, '', '/')
    if (useStore.getState().channels.some((channel) => channel.id === target)) {
      useStore.getState().setActiveChannel(target)
    }
  }, [])

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === 'k') {
        event.preventDefault()
        setSearchOpen(true)
      }
      if (event.key === 'Escape') {
        setSidebarOpen(false)
        setLightbox(null)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const openProfile = useCallback((userId: string) => setProfileId(userId), [])

  if (!me) {
    return (
      <div className="h-full flex items-center justify-center" style={{ color: 'var(--text-faint)' }}>
        <Spinner size={24} />
      </div>
    )
  }

  const showVoiceStage = voiceConnected && voiceChannelId === activeChannelId

  return (
    <div className="h-full flex overflow-hidden" style={{ background: 'var(--bg)' }}>
      {connection !== 'ready' && (
        <div
          className="fixed top-0 inset-x-0 z-50 text-center text-xs py-1.5 font-medium safe-top animate-fade-in"
          style={{ background: 'var(--warning)', color: '#10131c' }}
        >
          <WifiOff size={12} className="inline mr-1.5" />
          {connection === 'reconnecting' ? 'Reconnecting…' : 'Connecting…'}
        </div>
      )}

      {/* Sidebar — fixed on desktop, a drawer on mobile */}
      <aside className="hidden md:block w-60 shrink-0 border-r" style={{ borderColor: 'var(--border-soft)' }}>
        <Sidebar
          onOpenSettings={() => setSettingsOpen(true)}
          onOpenAdmin={() => setAdminOpen(true)}
          onOpenInvites={() => setInvitesOpen(true)}
          onCreateChannel={() => setCreateChannelOpen(true)}
          onOpenProfile={openProfile}
        />
      </aside>

      {sidebarOpen && (
        <div className="md:hidden fixed inset-0 z-40 flex">
          <div className="absolute inset-0 bg-black/55 animate-fade-in" onClick={() => setSidebarOpen(false)} />
          <div className="relative w-[82vw] max-w-72 h-full animate-slide-in-right" style={{ transform: 'none' }}>
            <Sidebar
              onOpenSettings={() => {
                setSidebarOpen(false)
                setSettingsOpen(true)
              }}
              onOpenAdmin={() => {
                setSidebarOpen(false)
                setAdminOpen(true)
              }}
              onOpenInvites={() => {
                setSidebarOpen(false)
                setInvitesOpen(true)
              }}
              onCreateChannel={() => {
                setSidebarOpen(false)
                setCreateChannelOpen(true)
              }}
              onOpenProfile={openProfile}
              onNavigate={() => setSidebarOpen(false)}
            />
          </div>
        </div>
      )}

      {/* Main column */}
      <main className="flex-1 flex flex-col min-w-0" style={{ background: 'var(--surface-1)' }}>
        <header
          className="h-14 px-3 sm:px-4 flex items-center gap-2 border-b shrink-0"
          style={{ borderColor: 'var(--border-soft)' }}
        >
          <button className="btn btn-ghost !p-1.5 md:hidden shrink-0" onClick={() => setSidebarOpen(true)} aria-label="Open channels">
            <Menu size={18} />
          </button>

          {channel ? (
            <>
              <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} size={17} />
              <h1 className="font-semibold truncate shrink-0">{channel.name}</h1>
              {channel.topic && (
                <>
                  <span className="w-px h-5 shrink-0 hidden sm:block" style={{ background: 'var(--border)' }} />
                  <p className="text-sm truncate hidden sm:block min-w-0" style={{ color: 'var(--text-muted)' }}>
                    {channel.topic}
                  </p>
                </>
              )}
              {channel.slowmode > 0 && (
                <span
                  className="text-[0.68rem] px-1.5 py-0.5 rounded shrink-0 hidden sm:inline"
                  style={{ background: 'var(--surface-3)', color: 'var(--text-muted)' }}
                >
                  slow {formatSlowmode(channel.slowmode)}
                </span>
              )}
            </>
          ) : (
            <h1 className="font-semibold" style={{ color: 'var(--text-muted)' }}>
              No channel selected
            </h1>
          )}

          <div className="flex-1" />

          {channel && channel.kind !== 'voice' && (
            <button className="btn btn-ghost !p-1.5 shrink-0" onClick={() => setPinsOpen(true)} title="Pinned messages">
              <Pin size={17} />
            </button>
          )}
          <button className="btn btn-ghost !p-1.5 shrink-0" onClick={() => setSearchOpen(true)} title="Search (Ctrl+K)">
            <Search size={17} />
          </button>
          <button
            className="btn btn-ghost !p-1.5 shrink-0"
            onClick={() => setMembersOpen((value) => !value)}
            title="Members"
            style={{ color: membersOpen ? 'var(--accent)' : undefined }}
          >
            <Users size={17} />
          </button>
        </header>

        {showVoiceStage && <VoiceStage />}

        {channel ? (
          channel.kind === 'voice' && !showVoiceStage ? (
            <div className="flex-1 flex items-center justify-center">
              <EmptyState
                icon={<Hash size={22} />}
                title={channel.name}
                body="Join this voice channel from the sidebar to talk, share your camera or share your screen."
              />
            </div>
          ) : channel.kind === 'voice' ? null : (
            <>
              <MessageList
                channel={channel}
                channelPermissions={perms}
                onReply={setReplyTo}
                onOpenProfile={openProfile}
                onOpenImage={setLightbox}
              />
              <Composer
                channel={channel}
                channelPermissions={perms}
                replyTo={replyTo}
                onCancelReply={() => setReplyTo(null)}
              />
            </>
          )
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <EmptyState
              icon={<Hash size={22} />}
              title="Pick a channel"
              body={
                can(permissions, P.MANAGE_CHANNELS)
                  ? "There's nothing here yet — create a channel to get started."
                  : 'Choose a channel from the sidebar to start chatting.'
              }
              action={
                can(permissions, P.MANAGE_CHANNELS) ? (
                  <button className="btn btn-primary" onClick={() => setCreateChannelOpen(true)}>
                    Create a channel
                  </button>
                ) : undefined
              }
            />
          </div>
        )}
      </main>

      {/* Member list */}
      {membersOpen && (
        <>
          <aside className="hidden xl:block w-60 shrink-0 border-l" style={{ borderColor: 'var(--border-soft)' }}>
            <MemberList onOpenProfile={openProfile} />
          </aside>
          <div className="xl:hidden fixed inset-0 z-40 flex justify-end">
            <div className="absolute inset-0 bg-black/55 animate-fade-in" onClick={() => setMembersOpen(false)} />
            <div className="relative w-[78vw] max-w-72 h-full animate-slide-in-right border-l">
              <MemberList onOpenProfile={openProfile} />
            </div>
          </div>
        </>
      )}

      {/* Overlays */}
      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <AdminPanel open={adminOpen} onClose={() => setAdminOpen(false)} onOpenProfile={openProfile} />
      <CreateChannelModal open={createChannelOpen} onClose={() => setCreateChannelOpen(false)} />
      <CreateInviteModal
        open={invitesOpen}
        onClose={() => setInvitesOpen(false)}
        canAssignRole={can(permissions, P.MANAGE_ROLES)}
      />
      <ProfileModal
        userId={profileId}
        onClose={() => setProfileId(null)}
        onEditProfile={() => setSettingsOpen(true)}
      />
      <SearchModal open={searchOpen} onClose={() => setSearchOpen(false)} onOpenProfile={openProfile} />
      {channel && <PinsModal open={pinsOpen} onClose={() => setPinsOpen(false)} channelId={channel.id} />}
      {lightbox && <Lightbox attachment={lightbox} onClose={() => setLightbox(null)} />}
    </div>
  )
}

function SearchModal({
  open,
  onClose,
  onOpenProfile,
}: {
  open: boolean
  onClose: () => void
  onOpenProfile: (userId: string) => void
}) {
  const channels = useStore((s) => s.channels)
  const jumpToMessage = useStore((s) => s.jumpToMessage)
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<Message[]>([])
  const [searching, setSearching] = useState(false)

  useEffect(() => {
    if (!open) {
      setQuery('')
      setResults([])
    }
  }, [open])

  useEffect(() => {
    if (query.trim().length < 2) {
      setResults([])
      return
    }
    setSearching(true)
    const id = setTimeout(() => {
      api
        .search({ q: query.trim() })
        .then(setResults)
        .catch(() => setResults([]))
        .finally(() => setSearching(false))
    }, 280)
    return () => clearTimeout(id)
  }, [query])

  return (
    <Modal open={open} onClose={onClose} width="md" bare>
      <div className="p-3 border-b">
        <div className="relative">
          <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-faint)' }} />
          <input
            className="input !pl-9"
            placeholder="Search messages…"
            value={query}
            autoFocus
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
      </div>

      <div className="max-h-96 overflow-y-auto scroll-thin">
        {searching && (
          <div className="flex justify-center py-8" style={{ color: 'var(--text-faint)' }}>
            <Spinner size={18} />
          </div>
        )}

        {!searching &&
          results.map((message) => {
            const channel = channels.find((c) => c.id === message.channel_id)
            return (
              <button
                key={message.id}
                className="w-full text-left px-4 py-3 border-b transition-colors hover:bg-[var(--surface-2)]"
                onClick={() => {
                  void jumpToMessage(message.channel_id, message.id)
                  onClose()
                }}
              >
                <div className="flex items-center gap-2 mb-1">
                  <span
                    className="text-xs font-semibold hover:underline"
                    onClick={(event) => {
                      event.stopPropagation()
                      if (message.author) onOpenProfile(message.author.id)
                    }}
                  >
                    {message.author?.display_name ?? message.webhook_name ?? 'Deleted member'}
                  </span>
                  <span className="text-[0.68rem]" style={{ color: 'var(--text-faint)' }}>
                    #{channel?.name ?? 'unknown'} · {formatTimestamp(message.created_at)}
                  </span>
                </div>
                <p className="text-sm line-clamp-2" style={{ color: 'var(--text-muted)' }}>
                  {message.content || 'Attachment'}
                </p>
              </button>
            )
          })}

        {!searching && query.trim().length >= 2 && !results.length && (
          <p className="text-sm text-center py-10" style={{ color: 'var(--text-faint)' }}>
            No messages matched “{query.trim()}”.
          </p>
        )}

        {query.trim().length < 2 && (
          <p className="text-sm text-center py-10" style={{ color: 'var(--text-faint)' }}>
            Type at least two characters to search every channel you can see.
          </p>
        )}
      </div>
    </Modal>
  )
}

function PinsModal({ open, onClose, channelId }: { open: boolean; onClose: () => void; channelId: string }) {
  const [pins, setPins] = useState<Message[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!open) return
    setLoading(true)
    api
      .pins(channelId)
      .then(setPins)
      .catch(() => setPins([]))
      .finally(() => setLoading(false))
  }, [open, channelId])

  return (
    <Modal open={open} onClose={onClose} title="Pinned messages" width="md">
      <div className="max-h-96 overflow-y-auto scroll-thin">
        {loading ? (
          <div className="flex justify-center py-10" style={{ color: 'var(--text-faint)' }}>
            <Spinner size={18} />
          </div>
        ) : pins.length ? (
          pins.map((message) => (
            <button
              key={message.id}
              className="w-full text-left px-5 py-3 border-b last:border-b-0 transition-colors hover:bg-[var(--surface-2)]"
              onClick={() => {
                void useStore.getState().jumpToMessage(message.channel_id, message.id)
                onClose()
              }}
            >
              <div className="flex items-center gap-2 mb-1">
                <span className="text-xs font-semibold">
                  {message.author?.display_name ?? message.webhook_name ?? 'Deleted member'}
                </span>
                <span className="text-[0.68rem]" style={{ color: 'var(--text-faint)' }}>
                  {formatTimestamp(message.created_at)}
                </span>
              </div>
              <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-muted)' }}>
                {message.content || 'Attachment'}
              </p>
            </button>
          ))
        ) : (
          <EmptyState icon={<Pin size={20} />} title="Nothing pinned" body="Pin an important message and it'll show up here." />
        )}
      </div>
    </Modal>
  )
}

function Lightbox({ attachment, onClose }: { attachment: Attachment; onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center p-4 animate-fade-in"
      style={{ background: 'rgb(0 0 0 / 0.86)' }}
      onClick={onClose}
    >
      <img
        src={attachment.url}
        alt={attachment.filename}
        className="max-w-full max-h-[86vh] object-contain rounded-lg animate-pop-in"
        onClick={(event) => event.stopPropagation()}
      />
      <div className="absolute top-4 right-4 flex gap-2">
        <a
          href={attachment.url}
          download={attachment.filename}
          className="btn btn-subtle"
          onClick={(event) => event.stopPropagation()}
        >
          <Download size={15} /> Download
        </a>
        <button className="btn btn-subtle !p-2" onClick={onClose} aria-label="Close">
          <X size={16} />
        </button>
      </div>
      <p className="absolute bottom-4 inset-x-0 text-center text-xs text-white/70 truncate px-4">
        {attachment.filename}
      </p>
    </div>
  )
}
