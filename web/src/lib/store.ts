import { create } from 'zustand'
import { api, ApiError, getToken, setToken } from './api'
import { gateway, type GatewayEvent, type GatewayStatus } from './gateway'
import { toBits } from './perms'
import { applyBranding, applyMetaBranding } from './theme'
import type {
  Category, Channel, Emoji, Instance, InstanceMeta, Me, Member, Message,
  NotificationPreferences, Relationship, Role, VoiceState,
} from './types'

export type AppPhase = 'loading' | 'setup' | 'anonymous' | 'ready' | 'error'

interface TypingEntry {
  userId: string
  displayName: string
  at: number
}

interface AppState {
  phase: AppPhase
  bootError: string

  meta: InstanceMeta | null
  instance: Instance | null
  me: Me | null
  permissions: bigint
  /** The viewer's effective permission bits per channel, from the server. */
  channelPermissions: Record<string, bigint>
  roles: Role[]
  categories: Category[]
  channels: Channel[]
  members: Record<string, Member>
  voiceStates: Record<string, VoiceState>
  voiceEnabled: boolean
  livekitUrl: string

  messages: Record<string, Message[]>
  hasMore: Record<string, boolean>
  loadingChannel: Record<string, boolean>
  unread: Record<string, number>
  /** Unread mentions per channel — tracked apart from unread messages so the
   *  sidebar can shout about one and stay quiet about the other. */
  mentionCounts: Record<string, number>
  lastRead: Record<string, string>
  typing: Record<string, TypingEntry[]>

  emojis: Emoji[]
  /** My friends, requests, blocks and favourites, keyed by the other member. */
  relationships: Record<string, Relationship>
  notifications: NotificationPreferences
  pushEnabled: boolean

  connection: GatewayStatus
  activeChannelId: string | null
  /** Set when the user asks to jump to a specific message; the message list
   *  consumes it, scrolls there and clears it. */
  pendingJump: { channelId: string; messageId: string } | null

  boot: () => Promise<void>
  finishAuth: (token: string) => Promise<void>
  logout: () => void
  setActiveChannel: (channelId: string | null) => void
  loadMessages: (channelId: string) => Promise<void>
  loadOlder: (channelId: string) => Promise<void>
  sendMessage: (channelId: string, content: string, replyTo: Message | null, attachments: unknown[]) => Promise<void>
  retryMessage: (channelId: string, messageId: string) => Promise<void>
  sendTyping: (channelId: string) => void
  markRead: (channelId: string) => void
  applyEvent: (event: GatewayEvent) => void
  patchMe: (me: Me) => void
  refreshMembers: () => Promise<void>
  refreshChannelPermissions: () => Promise<void>
  jumpToMessage: (channelId: string, messageId: string) => Promise<void>
  clearJump: () => void
  setNotifications: (preferences: NotificationPreferences) => void
  refreshRelationships: () => Promise<void>
}

const TYPING_TTL = 7000

function sortChannels(channels: Channel[]) {
  return [...channels].sort((a, b) => a.position - b.position || a.name.localeCompare(b.name))
}

function upsertMessage(list: Message[], message: Message): Message[] {
  const index = list.findIndex((m) => m.id === message.id)
  if (index >= 0) {
    const next = [...list]
    next[index] = { ...next[index], ...message, pending: false, failed: false }
    return next
  }
  // Messages carry sortable IDs, so appending then sorting keeps ordering
  // correct even if events arrive out of order.
  const next = [...list, message]
  next.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  return next
}

export const useStore = create<AppState>((set, get) => ({
  phase: 'loading',
  bootError: '',

  meta: null,
  instance: null,
  me: null,
  permissions: 0n,
  channelPermissions: {},
  roles: [],
  categories: [],
  channels: [],
  members: {},
  voiceStates: {},
  voiceEnabled: false,
  livekitUrl: '',

  messages: {},
  hasMore: {},
  loadingChannel: {},
  unread: {},
  mentionCounts: {},
  lastRead: {},
  typing: {},

  emojis: [],
  relationships: {},
  notifications: { mode: 'mentions', channels: [] },
  pushEnabled: false,

  connection: 'closed',
  activeChannelId: null,
  pendingJump: null,

  async boot() {
    try {
      const meta = await api.meta()
      applyMetaBranding(meta)
      set({ meta })

      if (!meta.setup_complete) {
        set({ phase: 'setup' })
        return
      }
      if (!getToken()) {
        set({ phase: 'anonymous' })
        return
      }

      // Validate the stored token before showing the app shell, so an expired
      // session lands on the login screen rather than an empty chat.
      try {
        await api.me()
      } catch (error) {
        if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
          setToken(null)
          set({ phase: 'anonymous' })
          return
        }
        throw error
      }

      gateway.connect()
      set({ phase: 'ready' })
    } catch (error) {
      set({
        phase: 'error',
        bootError: error instanceof Error ? error.message : 'Could not reach the server.',
      })
    }
  },

  async finishAuth(token: string) {
    setToken(token)
    const meta = await api.meta()
    applyMetaBranding(meta)
    set({ meta, phase: 'ready' })
    gateway.connect()
  },

  logout() {
    gateway.disconnect()
    setToken(null)
    set({
      phase: 'anonymous',
      me: null,
      permissions: 0n,
      channels: [],
      members: {},
      messages: {},
      activeChannelId: null,
      voiceStates: {},
    })
  },

  setActiveChannel(channelId) {
    set({ activeChannelId: channelId })
    if (!channelId) return
    try {
      localStorage.setItem('minichat.channel', channelId)
    } catch {
      /* ignore */
    }
    if (!get().messages[channelId]) void get().loadMessages(channelId)
    else get().markRead(channelId)
  },

  async loadMessages(channelId) {
    if (get().loadingChannel[channelId]) return
    set((s) => ({ loadingChannel: { ...s.loadingChannel, [channelId]: true } }))
    try {
      const messages = await api.messages(channelId, { limit: 50 })
      set((s) => ({
        messages: { ...s.messages, [channelId]: messages },
        hasMore: { ...s.hasMore, [channelId]: messages.length >= 50 },
      }))
      get().markRead(channelId)
    } catch {
      set((s) => ({ messages: { ...s.messages, [channelId]: s.messages[channelId] ?? [] } }))
    } finally {
      set((s) => ({ loadingChannel: { ...s.loadingChannel, [channelId]: false } }))
    }
  },

  async loadOlder(channelId) {
    const existing = get().messages[channelId] ?? []
    if (!existing.length || get().loadingChannel[channelId] || !get().hasMore[channelId]) return
    set((s) => ({ loadingChannel: { ...s.loadingChannel, [channelId]: true } }))
    try {
      const older = await api.messages(channelId, { before: existing[0].id, limit: 50 })
      set((s) => ({
        messages: { ...s.messages, [channelId]: [...older, ...(s.messages[channelId] ?? [])] },
        hasMore: { ...s.hasMore, [channelId]: older.length >= 50 },
      }))
    } finally {
      set((s) => ({ loadingChannel: { ...s.loadingChannel, [channelId]: false } }))
    }
  },

  async sendMessage(channelId, content, replyTo, attachments) {
    const me = get().me
    if (!me) return

    // Optimistic insert so the message appears instantly; the server echo
    // replaces it by ID.
    const tempId = `pending-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const optimistic: Message = {
      id: tempId,
      channel_id: channelId,
      author: {
        id: me.id,
        username: me.username,
        display_name: me.display_name,
        avatar_url: me.avatar_url,
        accent_color: me.accent_color,
        is_operator: me.is_operator,
        roles: me.roles,
      },
      content,
      reply_to_id: replyTo?.id ?? null,
      system_kind: null,
      webhook_name: null,
      pinned: false,
      created_at: new Date().toISOString(),
      edited_at: null,
      attachments: attachments as Message['attachments'],
      reactions: [],
      pending: true,
    }
    set((s) => ({
      messages: { ...s.messages, [channelId]: [...(s.messages[channelId] ?? []), optimistic] },
    }))

    try {
      const saved = await api.sendMessage(channelId, {
        content,
        reply_to_id: replyTo?.id ?? null,
        attachments,
      })
      set((s) => {
        const list = (s.messages[channelId] ?? []).filter((m) => m.id !== tempId)
        return { messages: { ...s.messages, [channelId]: upsertMessage(list, saved) } }
      })
    } catch (error) {
      set((s) => ({
        messages: {
          ...s.messages,
          [channelId]: (s.messages[channelId] ?? []).map((m) =>
            m.id === tempId
              ? { ...m, pending: false, failed: true, system_kind: null }
              : m,
          ),
        },
      }))
      throw error
    }
  },

  async retryMessage(channelId, messageId) {
    const message = (get().messages[channelId] ?? []).find((m) => m.id === messageId)
    if (!message) return
    set((s) => ({
      messages: {
        ...s.messages,
        [channelId]: (s.messages[channelId] ?? []).filter((m) => m.id !== messageId),
      },
    }))
    const replyTo = message.reply_to_id
      ? (get().messages[channelId] ?? []).find((m) => m.id === message.reply_to_id) ?? null
      : null
    await get().sendMessage(channelId, message.content, replyTo, message.attachments)
  },

  sendTyping(channelId) {
    gateway.send({ op: 'typing', channel_id: channelId })
  },

  markRead(channelId) {
    const list = get().messages[channelId] ?? []
    const last = list[list.length - 1]
    set((s) => ({
      unread: { ...s.unread, [channelId]: 0 },
      mentionCounts: { ...s.mentionCounts, [channelId]: 0 },
    }))
    if (!last || last.pending) return
    set((s) => ({ lastRead: { ...s.lastRead, [channelId]: last.id } }))
    gateway.send({ op: 'ack', channel_id: channelId, message_id: last.id })
  },

  applyEvent(event) {
    const { t, d } = event
    switch (t) {
      case 'READY': {
        const members: Record<string, Member> = {}
        for (const member of d.members as Member[]) members[member.id] = member
        const voiceStates: Record<string, VoiceState> = {}
        for (const vs of d.voice_states as VoiceState[]) voiceStates[vs.user_id] = vs
        const lastRead: Record<string, string> = {}
        for (const entry of d.read_state as { channel_id: string; last_read_id: string }[]) {
          lastRead[entry.channel_id] = entry.last_read_id
        }

        applyBranding(d.instance)
        const channels = sortChannels(d.channels)
        const channelPermissions: Record<string, bigint> = {}
        for (const [id, bits] of Object.entries(
          (d.channel_permissions ?? {}) as Record<string, string>,
        )) {
          channelPermissions[id] = toBits(bits)
        }

        // Restore the last channel when it still exists, otherwise fall back
        // to the first text channel.
        let active = get().activeChannelId
        if (!active) {
          try {
            active = localStorage.getItem('minichat.channel')
          } catch {
            active = null
          }
        }
        if (!active || !channels.some((c) => c.id === active)) {
          // Prefer the instance's system channel, then any regular text
          // channel — landing in a read-only announcements channel would look
          // like the app is broken.
          const system = d.instance.system_channel_id as string | null
          active =
            (system && channels.some((c) => c.id === system) ? system : null) ??
            channels.find((c) => c.kind === 'text')?.id ??
            channels.find((c) => c.kind !== 'voice')?.id ??
            null
        }

        set({
          phase: 'ready',
          instance: d.instance,
          me: d.me,
          permissions: toBits(d.permissions),
          channelPermissions,
          roles: d.roles,
          categories: d.categories,
          channels,
          members,
          voiceStates,
          unread: d.unread ?? {},
          mentionCounts: d.mentions ?? {},
          emojis: d.emojis ?? [],
          notifications: d.notifications ?? { mode: 'mentions', channels: [] },
          pushEnabled: Boolean(d.push_enabled),
          lastRead,
          voiceEnabled: d.voice_enabled,
          livekitUrl: d.livekit_url,
          activeChannelId: active,
        })
        if (active && !get().messages[active]) void get().loadMessages(active)
        void get().refreshRelationships()
        break
      }

      case 'INVALID_SESSION': {
        get().logout()
        break
      }

      case 'ACCESS_UPDATE': {
        const channels = sortChannels(d.channels as Channel[])
        const visible = new Set(channels.map(c => c.id))
        const active = visible.has(get().activeChannelId ?? '') ? get().activeChannelId : channels.find(c => c.kind === 'text')?.id ?? null
        set(s => ({
          channels, categories: d.categories, permissions: toBits(d.permissions),
          channelPermissions: Object.fromEntries(Object.entries(d.channel_permissions as Record<string,string>).map(([id,bits]) => [id,toBits(bits)])),
          activeChannelId: active,
          messages: Object.fromEntries(Object.entries(s.messages).filter(([id]) => visible.has(id))),
          voiceStates: Object.fromEntries(Object.entries(s.voiceStates).filter(([,vs]) => visible.has(vs.channel_id))),
        }))
        if (active && !get().messages[active]) void get().loadMessages(active)
        break
      }

      case 'MENTION_ADD': {
        const { channel_id } = d as { channel_id: string }
        if (get().activeChannelId === channel_id && document.visibilityState === 'visible') break
        set((s) => ({
          mentionCounts: {
            ...s.mentionCounts,
            [channel_id]: (s.mentionCounts[channel_id] ?? 0) + 1,
          },
        }))
        break
      }

      case 'EMOJI_CREATE':
      case 'EMOJI_UPDATE': {
        const emoji = d as Emoji
        set((s) => ({
          emojis: [...s.emojis.filter((e) => e.id !== emoji.id), emoji].sort((a, b) =>
            a.name.localeCompare(b.name),
          ),
        }))
        break
      }

      case 'EMOJI_DELETE': {
        set((s) => ({ emojis: s.emojis.filter((e) => e.id !== d.id) }))
        break
      }

      case 'MESSAGE_CREATE': {
        const message = d as Message
        const isActive = get().activeChannelId === message.channel_id
        const isMine = message.author?.id === get().me?.id
        set((s) => {
          const existing = s.messages[message.channel_id]
          // Only track channels already loaded; the rest refetch on open.
          const messages = existing
            ? { ...s.messages, [message.channel_id]: upsertMessage(existing, message) }
            : s.messages
          const unread =
            isActive || isMine
              ? s.unread
              : { ...s.unread, [message.channel_id]: (s.unread[message.channel_id] ?? 0) + 1 }
          const typing = {
            ...s.typing,
            [message.channel_id]: (s.typing[message.channel_id] ?? []).filter(
              (entry) => entry.userId !== message.author?.id,
            ),
          }
          return { messages, unread, typing }
        })
        if (isActive && document.visibilityState === 'visible') get().markRead(message.channel_id)
        break
      }

      case 'MESSAGE_UPDATE': {
        const message = d as Message
        set((s) => {
          const existing = s.messages[message.channel_id]
          if (!existing) return {}
          return { messages: { ...s.messages, [message.channel_id]: upsertMessage(existing, message) } }
        })
        break
      }

      case 'MESSAGE_DELETE': {
        set((s) => {
          const existing = s.messages[d.channel_id]
          if (!existing) return {}
          return {
            messages: {
              ...s.messages,
              [d.channel_id]: existing.filter((m) => m.id !== d.id),
            },
          }
        })
        break
      }

      case 'REACTION_UPDATE': {
        const { message_id, channel_id, emoji, user_id, added } = d
        const isMe = user_id === get().me?.id
        set((s) => {
          const existing = s.messages[channel_id]
          if (!existing) return {}
          return {
            messages: {
              ...s.messages,
              [channel_id]: existing.map((message) => {
                if (message.id !== message_id) return message
                const reactions = [...message.reactions]
                const index = reactions.findIndex((r) => r.emoji === emoji)
                if (added) {
                  if (index >= 0) {
                    reactions[index] = {
                      ...reactions[index],
                      count: reactions[index].count + 1,
                      me: reactions[index].me || isMe,
                    }
                  } else {
                    reactions.push({ emoji, count: 1, me: isMe })
                  }
                } else if (index >= 0) {
                  const count = reactions[index].count - 1
                  if (count <= 0) reactions.splice(index, 1)
                  else reactions[index] = { ...reactions[index], count, me: isMe ? false : reactions[index].me }
                }
                return { ...message, reactions }
              }),
            },
          }
        })
        break
      }

      case 'TYPING_START': {
        if (d.user_id === get().me?.id) break
        const entry: TypingEntry = {
          userId: d.user_id,
          displayName: d.display_name,
          at: Date.now(),
        }
        set((s) => {
          const existing = (s.typing[d.channel_id] ?? []).filter((e) => e.userId !== d.user_id)
          return { typing: { ...s.typing, [d.channel_id]: [...existing, entry] } }
        })
        break
      }

      case 'PRESENCE_UPDATE': {
        set((s) => {
          const member = s.members[d.user_id]
          if (!member) return {}
          return { members: { ...s.members, [d.user_id]: { ...member, presence: d.presence } } }
        })
        break
      }

      case 'RELATIONSHIPS_STALE': {
        void get().refreshRelationships()
        break
      }

      case 'MEMBER_ADD':
      case 'MEMBER_UPDATE': {
        const member = d as Member
        const isMe = get().me?.id === member.id
        set((s) => ({
          members: { ...s.members, [member.id]: member },
          me: s.me && s.me.id === member.id ? { ...s.me, ...member } : s.me,
        }))
        // Our own roles changing can change what we can do in every channel.
        if (isMe) schedulePermissionRefresh()
        break
      }

      case 'MEMBER_REMOVE': {
        set((s) => {
          const members = { ...s.members }
          delete members[d.id]
          return { members }
        })
        break
      }

      case 'CHANNEL_CREATE':
      case 'CHANNEL_UPDATE': {
        set((s) => {
          const others = s.channels.filter((c) => c.id !== d.id)
          return { channels: sortChannels([...others, d as Channel]) }
        })
        schedulePermissionRefresh()
        break
      }

      case 'CHANNEL_DELETE': {
        set((s) => {
          const channels = s.channels.filter((c) => c.id !== d.id)
          const messages = { ...s.messages }
          delete messages[d.id]
          return {
            channels,
            messages,
            activeChannelId:
              s.activeChannelId === d.id
                ? channels.find((c) => c.kind !== 'voice')?.id ?? null
                : s.activeChannelId,
          }
        })
        break
      }

      case 'CATEGORY_CREATE':
      case 'CATEGORY_UPDATE': {
        set((s) => {
          const others = s.categories.filter((c) => c.id !== d.id)
          return {
            categories: [...others, d as Category].sort(
              (a, b) => a.position - b.position || a.name.localeCompare(b.name),
            ),
          }
        })
        break
      }

      case 'CATEGORY_DELETE': {
        set((s) => ({ categories: s.categories.filter((c) => c.id !== d.id) }))
        break
      }

      case 'ROLE_CREATE':
      case 'ROLE_UPDATE': {
        set((s) => {
          const others = s.roles.filter((r) => r.id !== d.id)
          return { roles: [...others, d as Role].sort((a, b) => b.position - a.position) }
        })
        schedulePermissionRefresh()
        break
      }

      case 'ROLE_DELETE': {
        set((s) => ({ roles: s.roles.filter((r) => r.id !== d.id) }))
        break
      }

      case 'INSTANCE_UPDATE': {
        applyBranding(d as Instance)
        set({ instance: d as Instance })
        break
      }

      case 'VOICE_STATE_UPDATE': {
        const vs = d as VoiceState
        set((s) => ({ voiceStates: { ...s.voiceStates, [vs.user_id]: vs } }))
        break
      }

      case 'VOICE_STATE_LEAVE': {
        set((s) => {
          const voiceStates = { ...s.voiceStates }
          delete voiceStates[d.user_id]
          return { voiceStates }
        })
        break
      }
    }
  },

  patchMe(me) {
    set((s) => ({
      me,
      members: { ...s.members, [me.id]: { ...s.members[me.id], ...me } },
    }))
  },

  /**
   * Jump to a message that may not be in the loaded page. Fetches a window
   * centred on it when needed, then hands the ID to the message list.
   */
  async jumpToMessage(channelId, messageId) {
    const loaded = get().messages[channelId] ?? []
    if (get().activeChannelId !== channelId) set({ activeChannelId: channelId })

    if (!loaded.some((m) => m.id === messageId)) {
      try {
        const window = await api.messages(channelId, { around: messageId, limit: 50 })
        set((s) => ({
          messages: { ...s.messages, [channelId]: window },
          // The window is a slice out of the middle, so older messages almost
          // certainly exist above it.
          hasMore: { ...s.hasMore, [channelId]: window.length > 0 },
        }))
      } catch {
        return
      }
    }
    set({ pendingJump: { channelId, messageId } })
  },

  clearJump() {
    set({ pendingJump: null })
  },

  /**
   * Reload the friends list.
   *
   * The server only says "this is stale" rather than sending the new list,
   * because a relationship event goes to both people and the two of them see
   * different sides of the same pair.
   */
  async refreshRelationships() {
    try {
      const list = await api.relationships()
      const map: Record<string, Relationship> = {}
      for (const entry of list) map[entry.user_id] = entry
      set({ relationships: map })
    } catch {
      /* a failed refresh just leaves the last known list in place */
    }
  },

  setNotifications(preferences) {
    set({ notifications: preferences })
  },

  async refreshChannelPermissions() {
    try {
      const map = await api.channelPermissions()
      const parsed: Record<string, bigint> = {}
      for (const [id, bits] of Object.entries(map)) parsed[id] = toBits(bits)
      set({ channelPermissions: parsed })
    } catch {
      /* keep the previous map rather than locking the UI down on a hiccup */
    }
  },

  async refreshMembers() {
    const list = await api.members()
    const members: Record<string, Member> = {}
    for (const member of list) members[member.id] = member
    set({ members })
  },
}))

/**
 * Permission changes often arrive as a burst of events; coalesce them into one
 * refresh so a role edit doesn't trigger a dozen requests.
 */
let permissionRefreshTimer: number | null = null
function schedulePermissionRefresh() {
  if (permissionRefreshTimer !== null) return
  permissionRefreshTimer = window.setTimeout(() => {
    permissionRefreshTimer = null
    void useStore.getState().refreshChannelPermissions()
  }, 350)
}

/** Drop typing indicators that have gone stale. */
export function pruneTyping() {
  const now = Date.now()
  const state = useStore.getState()
  let changed = false
  const typing: Record<string, TypingEntry[]> = {}
  for (const [channelId, entries] of Object.entries(state.typing)) {
    const kept = entries.filter((entry) => now - entry.at < TYPING_TTL)
    if (kept.length !== entries.length) changed = true
    if (kept.length) typing[channelId] = kept
  }
  if (changed) useStore.setState({ typing })
}

gateway.on((event) => useStore.getState().applyEvent(event))
gateway.onStatus((connection) => useStore.setState({ connection }))
