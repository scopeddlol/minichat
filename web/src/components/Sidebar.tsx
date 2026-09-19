import { moveChannel } from '../lib/channelOrder'
import {
  MessageSquare, ChevronDown, FolderPlus, Hash, Link2, Lock, Megaphone, MicOff, Pencil, Plus,
  ScreenShare, Settings, Shield, Sparkles, Trash2, UserPlus, Video, Volume2, VolumeX,
  PhoneOff, Users,
} from 'lucide-react'
import { useMemo, useRef, useState } from 'react'
import VoiceDeviceControl from './VoiceDeviceControl'
import { useInbox } from '../lib/direct'
import { api } from '../lib/api'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import { useVoice } from '../lib/voice'
import type { Category, Channel } from '../lib/types'
import { useContextMenu, useLongPress, type MenuItem } from './ContextMenu'
import { Avatar, copyText, toast, useConfirm } from './ui'

interface SidebarProps {
  onPickScreenShare: () => void
  onOpenSettings: () => void
  onOpenAdmin: () => void
  onOpenInvites: () => void
  /** Create a channel, optionally inside a category. */
  onCreateChannel: (categoryId?: string) => void
  onEditChannel: (channel: Channel) => void
  onCreateCategory: () => void
  onEditCategory: (category: Category) => void
  onOpenProfile: (userId: string) => void
  onNavigate?: () => void
}

export default function Sidebar({
  onPickScreenShare,
  onOpenSettings,
  onOpenAdmin,
  onOpenInvites,
  onCreateChannel,
  onEditChannel,
  onCreateCategory,
  onEditCategory,
  onOpenProfile,
  onNavigate,
}: SidebarProps) {
  const dragId = useRef<string | null>(null)
  const [dropTarget, setDropTarget] = useState<string | null>(null)
  const [reordering, setReordering] = useState(false)
  const reorder = async (category: string | null, target: string | null) => {
    const id = dragId.current
    dragId.current = null
    setDropTarget(null)
    if (!id || !canManageChannels || reordering || id === target) return
    const ordered = moveChannel(useStore.getState().channels, id, category, target)
    setReordering(true)
    try {
      await api.reorderChannels(ordered.map(c => ({ id: c.id, category_id: c.category_id, position: c.position })))
      // Gateway snapshots remain authoritative, including inherited permissions.
    } catch (e) { toast.error(e instanceof Error ? e.message : 'Could not reorder channels.') }
    finally { setReordering(false) }
  }
  const inboxOpen = useInbox(s => s.open)
  const conversations = useInbox(s => s.conversations)
  const unreadDirect = conversations.reduce((n,c) => n + c.unread, 0)
  const instance = useStore((s) => s.instance)
  const channels = useStore((s) => s.channels)
  const categories = useStore((s) => s.categories)
  const permissions = useStore((s) => s.permissions)
  const activeChannelId = useStore((s) => s.activeChannelId)
  const setActiveChannel = useStore((s) => s.setActiveChannel)
  const unread = useStore((s) => s.unread)
  const mentionCounts = useStore((s) => s.mentionCounts)
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})

  /** Right-click on a channel row. */
  const channelMenu = (channel: Channel): MenuItem[] => {
    const items: MenuItem[] = [
      {
        label: 'Copy link',
        icon: <Link2 size={14} />,
        onSelect: () => void copyText(`${location.origin}/?channel=${channel.id}`, 'Channel link copied'),
      },
    ]
    if (canManageChannels) {
      items.push(
        { separator: true },
        { label: 'Edit channel', icon: <Pencil size={14} />, onSelect: () => onEditChannel(channel) },
        {
          label: 'Create channel here',
          icon: <Plus size={14} />,
          onSelect: () => onCreateChannel(channel.category_id ?? undefined),
        },
        { separator: true },
        {
          label: 'Delete channel',
          icon: <Trash2 size={14} />,
          danger: true,
          onSelect: async () => {
            const ok = await confirm({
              title: `Delete ${channel.name}?`,
              body: 'Its messages go with it. This cannot be undone.',
              confirmLabel: 'Delete',
              danger: true,
            })
            if (!ok) return
            try {
              await api.deleteChannel(channel.id)
            } catch {
              toast.error('Could not delete the channel.')
            }
          },
        },
      )
    }
    return items
  }

  /** Right-click on a category heading. */
  const categoryMenu = (category: Category): MenuItem[] => {
    if (!canManageChannels) return []
    return [
      {
        label: 'Create channel here',
        icon: <Plus size={14} />,
        onSelect: () => onCreateChannel(category.id),
      },
      { label: 'Edit category', icon: <Pencil size={14} />, onSelect: () => onEditCategory(category) },
      { separator: true },
      {
        label: 'Delete category',
        icon: <Trash2 size={14} />,
        danger: true,
        onSelect: async () => {
          const ok = await confirm({
            title: `Delete ${category.name}?`,
            body: 'Its channels stay, but become uncategorised.',
            confirmLabel: 'Delete',
            danger: true,
          })
          if (!ok) return
          try {
            await api.deleteCategory(category.id)
          } catch {
            toast.error('Could not delete the category.')
          }
        },
      },
    ]
  }

  /** Right-click on empty space in the channel list. */
  const listMenu = (): MenuItem[] => {
    if (!canManageChannels) return []
    return [
      { label: 'Create channel', icon: <Plus size={14} />, onSelect: () => onCreateChannel() },
      { label: 'Create category', icon: <FolderPlus size={14} />, onSelect: onCreateCategory },
    ]
  }

  const grouped = useMemo(() => {
    const byCategory = new Map<string, Channel[]>()
    const uncategorised: Channel[] = []
    for (const channel of channels) {
      if (channel.category_id && categories.some((c) => c.id === channel.category_id)) {
        const list = byCategory.get(channel.category_id) ?? []
        list.push(channel)
        byCategory.set(channel.category_id, list)
      } else {
        uncategorised.push(channel)
      }
    }
    return { byCategory, uncategorised }
  }, [channels, categories])

  const menu = useContextMenu()
  const confirm = useConfirm()

  const canManageChannels = can(permissions, P.MANAGE_CHANNELS)
  const canInvite = can(permissions, P.CREATE_INVITES)
  const canAdmin =
    can(permissions, P.MANAGE_INSTANCE) ||
    can(permissions, P.MANAGE_ROLES) ||
    can(permissions, P.KICK_MEMBERS) ||
    can(permissions, P.VIEW_AUDIT_LOG) ||
    can(permissions, P.MANAGE_WEBHOOKS)

  const select = (channel: Channel) => {
    useInbox.setState({ open: false })
    if (channel.kind === 'voice') {
      void joinVoice(channel)
      return
    }
    setActiveChannel(channel.id)
    onNavigate?.()
  }

  const joinVoice = async (channel: Channel) => {
    const voice = useVoice.getState()
    if (voice.channelId === channel.id) {
      setActiveChannel(channel.id)
      onNavigate?.()
      return
    }
    if (!useStore.getState().voiceEnabled) {
      toast.error('Voice and video are not configured on this instance.')
      return
    }
    setActiveChannel(channel.id)
    onNavigate?.()
    try {
      await voice.join(channel.id)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Could not join the voice channel.')
    }
  }

  return (
    <div className="community-nav h-full min-h-0 flex flex-col" style={{ background: 'var(--surface-0)' }}>
      {/* Instance header */}
      <header
        className="px-3.5 h-14 flex items-center gap-2.5 border-b shrink-0"
        style={{ borderColor: 'var(--border-soft)' }}
      >
        {instance?.icon_url ? (
          <img src={instance.icon_url} alt="" className="w-8 h-8 rounded-lg object-cover shrink-0" />
        ) : (
          <div
            className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
            style={{ background: 'var(--accent)' }}
          >
            <Sparkles size={15} style={{ color: 'var(--accent-ink)' }} />
          </div>
        )}
        <div className="min-w-0 flex-1">
          <h1 className="font-semibold text-sm truncate leading-tight">{instance?.name ?? 'MiniChat'}</h1>
          {instance?.tagline && (
            <p className="text-[0.7rem] truncate" style={{ color: 'var(--text-faint)' }}>
              {instance.tagline}
            </p>
          )}
        </div>
        {canInvite && (
          <button className="btn btn-ghost !p-1.5" onClick={onOpenInvites} title="Invite people">
            <UserPlus size={16} />
          </button>
        )}
      </header>

      <div className="px-3 pt-4"><button className={`inbox-button ${inboxOpen ? 'selected' : ''}`} onClick={() => { useInbox.setState({ open: true }); onNavigate?.() }}><MessageSquare size={18}/><span className="flex-1 text-left">Direct messages</span>{unreadDirect > 0 && <span className="unread-dot">{unreadDirect > 99 ? '99+' : unreadDirect}</span>}</button><p className="navigation-label">COMMUNITY</p></div>
      {/* Channels */}
      <nav
        className="flex-1 min-h-0 overflow-y-auto scroll-thin px-2 py-3 space-y-4"
        aria-label="Community channels"
        onDragStart={event => {
          const row = (event.target as HTMLElement).closest<HTMLElement>('[data-channel-id]')
          if (!canManageChannels || reordering || !row) { event.preventDefault(); return }
          dragId.current = row.dataset.channelId ?? null
          event.dataTransfer.effectAllowed = 'move'
          event.dataTransfer.setData('text/plain', dragId.current ?? '')
        }}
        onDragOver={event => {
          if (!dragId.current) return
          event.preventDefault()
          event.dataTransfer.dropEffect = 'move'
          const row = (event.target as HTMLElement).closest<HTMLElement>('[data-channel-id]')
          setDropTarget(row?.dataset.channelId ?? null)
        }}
        onDragEnd={() => { dragId.current = null; setDropTarget(null) }}
        onDrop={event => {
          if (!dragId.current) return
          event.preventDefault()
          const row = (event.target as HTMLElement).closest<HTMLElement>('[data-channel-id]')
          const target = channels.find(c => c.id === row?.dataset.channelId)
          const group = (event.target as HTMLElement).closest<HTMLElement>('[data-category-id]')
          void reorder(target?.category_id ?? group?.dataset.categoryId ?? null, target?.id ?? null)
        }}
        onContextMenu={(event) => {
          // Row handlers prevent default; unused padding inside groups also
          // belongs to the channel-list menu.
          if (event.defaultPrevented) return
          menu.open(event, listMenu())
        }}
      >
        {grouped.uncategorised.length > 0 && (
          <ChannelGroup
            channels={grouped.uncategorised}
            activeChannelId={inboxOpen ? null : activeChannelId}
            unread={unread}
            mentionCounts={mentionCounts}
            onSelect={select}
            onOpenProfile={onOpenProfile}
            buildMenu={channelMenu}
            draggable={canManageChannels && !reordering}
            dropTarget={dropTarget}
          />
        )}

        {categories.map((category) => {
          const list = grouped.byCategory.get(category.id) ?? []
          if (!list.length && !canManageChannels) return null
          const isCollapsed = collapsed[category.id]
          return (
            <div key={category.id} data-category-id={category.id}>
              <button
                className="w-full flex items-center gap-1 px-1.5 mb-1 group"
                onClick={() => setCollapsed((value) => ({ ...value, [category.id]: !value[category.id] }))}
                onKeyDown={event => {
                  if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
                  event.preventDefault()
                  const rect = event.currentTarget.getBoundingClientRect()
                  menu.open({ clientX: rect.left, clientY: rect.bottom, preventDefault() {} }, categoryMenu(category))
                }}
                onContextMenu={(event) => menu.open(event, categoryMenu(category))}
              >
                <ChevronDown
                  size={12}
                  className="transition-transform shrink-0"
                  style={{
                    color: 'var(--text-faint)',
                    transform: isCollapsed ? 'rotate(-90deg)' : 'none',
                  }}
                />
                <span
                  className="text-[0.68rem] font-bold uppercase tracking-wider truncate transition-colors group-hover:text-[var(--text)]"
                  style={{ color: 'var(--text-faint)' }}
                >
                  {category.name}
                </span>
                {category.is_private && (
                  <Lock
                    size={10}
                    className="shrink-0"
                    style={{ color: 'var(--text-faint)' }}
                    aria-label="Private category"
                  />
                )}
              </button>
              {!isCollapsed && (
                <ChannelGroup
                  channels={list}
                  activeChannelId={inboxOpen ? null : activeChannelId}
                  unread={unread}
                  mentionCounts={mentionCounts}
                  onSelect={select}
                  onOpenProfile={onOpenProfile}
                  buildMenu={channelMenu}
            draggable={canManageChannels && !reordering}
            dropTarget={dropTarget}
                />
              )}
            </div>
          )
        })}

        {canManageChannels && (
          <button
            className="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-sm transition-colors hover:bg-[var(--surface-2)]"
            style={{ color: 'var(--text-faint)' }}
            onClick={() => onCreateChannel()}
          >
            <Plus size={15} />
            Create channel
          </button>
        )}

        {!channels.length && (
          <p className="text-xs px-2 py-4 text-center" style={{ color: 'var(--text-faint)' }}>
            No channels you can see yet.
          </p>
        )}
      </nav>

      <VoiceDock onPickScreenShare={onPickScreenShare} />

      <UserDock
        onOpenSettings={onOpenSettings}
        onOpenAdmin={canAdmin ? onOpenAdmin : undefined}
        onOpenProfile={onOpenProfile}
      />
    </div>
  )
}

/**
 * One channel in the sidebar.
 *
 * Exists as a component so each row can own a long-press timer of its own —
 * touch screens have no right-click, and hooks can't be called inside the map
 * that renders the list.
 */
function ChannelRow({
  channel,
  active,
  unreadCount,
  buildMenu,
  onSelect,
  children,
  draggable,
  dropTarget,
}: {
  draggable: boolean
  dropTarget: string | null
  channel: Channel
  active: boolean
  /** Drives the bold-and-brighter treatment an unread channel gets. */
  unreadCount: number
  buildMenu: (channel: Channel) => MenuItem[]
  onSelect: (channel: Channel) => void
  children: React.ReactNode
}) {
  const menu = useContextMenu()
  const longPress = useLongPress((point) =>
    menu.open({ ...point, preventDefault: () => undefined }, buildMenu(channel)),
  )

  return (
    <button
      draggable={draggable}
      aria-current={active ? "page" : undefined}
      data-channel-id={channel.id}
      onClick={() => onSelect(channel)}
      onKeyDown={event => {
        if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
        event.preventDefault()
        const rect = event.currentTarget.getBoundingClientRect()
        menu.open({ clientX: rect.left, clientY: rect.bottom, preventDefault() {} }, buildMenu(channel))
      }}
      onContextMenu={(event) => menu.open(event, buildMenu(channel))}
      {...longPress}
      className="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-sm transition-colors group"
      style={{
        boxShadow: dropTarget === channel.id ? 'inset 0 2px var(--accent)' : undefined,
        background: active ? 'var(--surface-2)' : 'transparent',
        color: active || unreadCount > 0 ? 'var(--text)' : 'var(--text-muted)',
        fontWeight: unreadCount > 0 ? 600 : 500,
      }}
      onMouseEnter={(event) => {
        if (!active) event.currentTarget.style.background = 'var(--surface-1)'
      }}
      onMouseLeave={(event) => {
        if (!active) event.currentTarget.style.background = 'transparent'
      }}
    >
      {children}
    </button>
  )
}

function ChannelGroup({
  channels,
  activeChannelId,
  unread,
  mentionCounts,
  onSelect,
  onOpenProfile,
  buildMenu,
  draggable,
  dropTarget,
}: {
  draggable: boolean
  dropTarget: string | null
  channels: Channel[]
  activeChannelId: string | null
  unread: Record<string, number>
  mentionCounts: Record<string, number>
  onSelect: (channel: Channel) => void
  onOpenProfile: (userId: string) => void
  buildMenu: (channel: Channel) => MenuItem[]
}) {
  const voiceStates = useStore((s) => s.voiceStates)
  const members = useStore((s) => s.members)

  return (
    <div className="space-y-0.5">
      {channels.map((channel) => {
        const active = activeChannelId === channel.id
        const count = unread[channel.id] ?? 0
        const mentions = mentionCounts[channel.id] ?? 0
        const occupants = Object.values(voiceStates).filter((vs) => vs.channel_id === channel.id)

        return (
          <div key={channel.id}>
            <ChannelRow
              draggable={draggable}
              dropTarget={dropTarget}
              channel={channel}
              active={active}
              unreadCount={count}
              buildMenu={buildMenu}
              onSelect={onSelect}
            >
              <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} emoji={channel.emoji} />
              <span className="flex-1 min-w-0 text-left">
                <span className="block truncate">{channel.name}</span>
                {channel.description && (
                  <span
                    className="block truncate text-[0.68rem] font-normal leading-tight"
                    style={{ color: 'var(--text-faint)' }}
                  >
                    {channel.description}
                  </span>
                )}
              </span>
              {channel.kind === 'voice' && occupants.length > 0 && (
                <span className="text-[0.68rem] tabular-nums" style={{ color: 'var(--text-faint)' }}>
                  {occupants.length}
                  {channel.user_limit > 0 ? `/${channel.user_limit}` : ''}
                </span>
              )}
              {/* A mention badge outranks a plain unread count — being named
                  is a different signal from a busy channel. */}
              {mentions > 0 && channel.kind !== 'voice' ? (
                <span
                  className="text-[0.66rem] font-bold px-1.5 rounded-full tabular-nums shrink-0"
                  style={{ background: 'var(--danger)', color: '#fff', minWidth: 18, lineHeight: '17px' }}
                  title={`${mentions} mention${mentions === 1 ? '' : 's'}`}
                >
                  @{mentions > 9 ? '9+' : mentions}
                </span>
              ) : count > 0 && channel.kind !== 'voice' ? (
                <span
                  className="text-[0.66rem] font-bold px-1.5 rounded-full tabular-nums shrink-0"
                  style={{ background: 'var(--accent)', color: 'var(--accent-ink)', minWidth: 18, lineHeight: '17px' }}
                >
                  {count > 99 ? '99+' : count}
                </span>
              ) : null}
            </ChannelRow>

            {/* Who's in this voice channel */}
            {channel.kind === 'voice' && occupants.length > 0 && (
              <div className="ml-4 pl-2 mt-0.5 mb-1 space-y-0.5 border-l" style={{ borderColor: 'var(--border-soft)' }}>
                {occupants.map((vs) => {
                  const member = members[vs.user_id]
                  if (!member) return null
                  return (
                    <button
                      key={vs.user_id}
                      data-user-id={vs.user_id}
                      onClick={() => onOpenProfile(vs.user_id)}
                      className="w-full flex items-center gap-2 px-1.5 py-1 rounded-md transition-colors hover:bg-[var(--surface-1)]"
                    >
                      <Avatar
                        id={member.id}
                        name={member.display_name}
                        src={member.avatar_url}
                        frame={member.avatar_frame}
                        accent={member.accent_color}
                        size="xs"
                      />
                      <span
                        className="text-[0.78rem] truncate flex-1 text-left"
                        style={{ color: vs.muted ? 'var(--text-faint)' : 'var(--text-muted)' }}
                      >
                        {member.display_name}
                      </span>
                      <span className="flex items-center gap-1 shrink-0" style={{ color: 'var(--text-faint)' }}>
                        {vs.streaming && <ScreenShare size={11} style={{ color: 'var(--success)' }} />}
                        {vs.video && <Video size={11} style={{ color: 'var(--success)' }} />}
                        {vs.deafened ? (
                          <VolumeX size={11} style={{ color: 'var(--danger)' }} />
                        ) : vs.muted ? (
                          <MicOff size={11} style={{ color: 'var(--danger)' }} />
                        ) : null}
                      </span>
                    </button>
                  )
                })}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}

export function ChannelIcon({
  kind,
  isPrivate,
  emoji,
  size = 15,
}: {
  kind: string
  isPrivate?: boolean
  /** Replaces the glyph entirely when the channel has one set. */
  emoji?: string
  size?: number
}) {
  const style = { color: 'var(--text-faint)' } as const
  if (emoji && emoji.trim()) {
    return (
      <span
        className="shrink-0 inline-flex items-center justify-center leading-none"
        style={{ width: size, height: size, fontSize: size * 0.95 }}
        aria-hidden
      >
        {emoji.trim()}
      </span>
    )
  }
  if (kind === 'voice') return <Volume2 size={size} style={style} className="shrink-0" />
  if (kind === 'announcement') return <Megaphone size={size} style={style} className="shrink-0" />
  if (isPrivate) return <Shield size={size} style={style} className="shrink-0" />
  return <Hash size={size} style={style} className="shrink-0" />
}

/** Live voice controls, shown only while connected to a channel. */
function VoiceDock({ onPickScreenShare }: { onPickScreenShare: () => void }) {
  const {
    connected, connecting, channelId, cameraOn, screenSharing,
    canVideo, canScreenShare, pushToTalkActive,
  } = useVoice()
  const voice = useVoice()
  const channels = useStore((s) => s.channels)
  const channel = channels.find((c) => c.id === channelId)

  if (!connected && !connecting) return null

  return (
    <div
      className="px-3 py-2.5 border-t animate-fade-up"
      style={{ borderColor: 'var(--border-soft)', background: 'var(--surface-1)' }}
    >
      <div className="flex items-center gap-2 mb-2">
        <span
          className="w-2 h-2 rounded-full shrink-0"
          style={{ background: connected ? 'var(--success)' : 'var(--warning)' }}
        />
        <div className="min-w-0 flex-1">
          <p className="text-[0.78rem] font-semibold truncate" style={{ color: connected ? 'var(--success)' : 'var(--warning)' }}>
            {connecting ? 'Connecting…' : 'Voice connected'}
          </p>
          <p className="text-[0.7rem] truncate" style={{ color: 'var(--text-faint)' }}>
            {pushToTalkActive ? 'Transmitting…' : (channel?.name ?? 'Voice channel')}
          </p>
        </div>
        <button
          className="btn btn-ghost !p-1.5"
          onClick={() => void voice.leave()}
          title="Disconnect"
          style={{ color: 'var(--danger)' }}
        >
          <PhoneOff size={15} />
        </button>
      </div>

      <div className="grid grid-cols-4 gap-1.5">
        <VoiceDeviceControl kind="audioinput" />
        <VoiceDeviceControl kind="audiooutput" />
        <VoiceButton
          active={cameraOn}
          disabled={!canVideo}
          onClick={() => void voice.toggleCamera()}
          title={cameraOn ? 'Stop camera' : 'Start camera'}
        >
          <Video size={15} />
        </VoiceButton>
        <VoiceButton
          active={screenSharing}
          disabled={!canScreenShare}
          onClick={() => {
            // Already sharing: stop straight away. Otherwise ask for quality
            // first, since it can't be changed mid-share.
            if (screenSharing) void voice.stopScreenShare()
            else onPickScreenShare()
          }}
          title={screenSharing ? 'Stop sharing' : 'Share screen'}
        >
          <ScreenShare size={15} />
        </VoiceButton>
      </div>
    </div>
  )
}

function VoiceButton({
  children,
  active,
  danger,
  disabled,
  live,
  onClick,
  title,
}: {
  children: React.ReactNode
  active?: boolean
  danger?: boolean
  disabled?: boolean
  /** Push-to-talk is currently open. */
  live?: boolean
  onClick: () => void
  title: string
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={title}
      className={`flex items-center justify-center py-1.5 rounded-lg transition-colors disabled:opacity-35 disabled:cursor-not-allowed${live ? ' speaking-ring' : ''}`}
      style={{
        background: live
          ? 'color-mix(in oklab, var(--success) 22%, transparent)'
          : danger
            ? 'color-mix(in oklab, var(--danger) 18%, transparent)'
            : active
              ? 'var(--accent-soft)'
              : 'var(--surface-2)',
        color: live
          ? 'var(--success)'
          : danger
            ? 'var(--danger)'
            : active
              ? 'var(--accent)'
              : 'var(--text-muted)',
      }}
    >
      {children}
    </button>
  )
}

function UserDock({
  onOpenSettings,
  onOpenAdmin,
  onOpenProfile,
}: {
  onOpenSettings: () => void
  onOpenAdmin?: () => void
  onOpenProfile: (userId: string) => void
}) {
  const me = useStore((s) => s.me)
  const connection = useStore((s) => s.connection)
  if (!me) return null

  return (
    <footer
      className="px-2.5 py-2 border-t flex items-center gap-2 shrink-0 safe-bottom"
      style={{ borderColor: 'var(--border-soft)', background: 'var(--surface-1)' }}
    >
      <button
        className="flex items-center gap-2.5 min-w-0 flex-1 px-1.5 py-1 rounded-lg transition-colors hover:bg-[var(--surface-2)]"
        onClick={() => onOpenProfile(me.id)}
      >
        <Avatar
          id={me.id}
          name={me.display_name}
          src={me.avatar_url}
          frame={me.avatar_frame}
          accent={me.accent_color}
          size="md"
          presence={connection === 'ready' ? me.presence : 'offline'}
        />
        <div className="min-w-0 text-left">
          <p className="text-[0.82rem] font-semibold truncate leading-tight">{me.display_name}</p>
          <p className="text-[0.7rem] truncate" style={{ color: 'var(--text-faint)' }}>
            {connection === 'ready'
              ? me.custom_status || `@${me.username}`
              : connection === 'reconnecting'
                ? 'Reconnecting…'
                : 'Offline'}
          </p>
        </div>
      </button>

      {onOpenAdmin && (
        <button className="btn btn-ghost !p-1.5 shrink-0" onClick={onOpenAdmin} title="Admin panel">
          <Shield size={16} />
        </button>
      )}
      <button className="btn btn-ghost !p-1.5 shrink-0" onClick={onOpenSettings} title="Settings">
        <Settings size={16} />
      </button>
    </footer>
  )
}

export { Users }
