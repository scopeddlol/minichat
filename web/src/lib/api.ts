import type {
  Attachment, AuditEntry, BanEntry, Category, Channel, Instance, Invite,
  InvitePreview, InstanceMeta, Me, Member, Message, PermissionDef, Role, Stats,
  VoiceState, Webhook,
} from './types'

const TOKEN_KEY = 'minichat.token'

export class ApiError extends Error {
  status: number
  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export function setToken(token: string | null) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token)
    else localStorage.removeItem(TOKEN_KEY)
  } catch {
    /* private browsing — the session simply won't persist */
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getToken()
  const headers = new Headers(init.headers)
  if (token) headers.set('Authorization', `Bearer ${token}`)
  if (init.body && !(init.body instanceof FormData)) {
    headers.set('Content-Type', 'application/json')
  }

  let response: Response
  try {
    response = await fetch(`/api${path}`, { ...init, headers })
  } catch {
    throw new ApiError('Could not reach the server. Check your connection.', 0)
  }

  if (response.status === 204) return undefined as T

  const text = await response.text()
  let data: unknown = null
  if (text) {
    try {
      data = JSON.parse(text)
    } catch {
      data = null
    }
  }

  if (!response.ok) {
    const detail =
      data && typeof data === 'object' && 'error' in data
        ? String((data as { error: unknown }).error)
        : ''
    throw new ApiError(detail || `Request failed (${response.status})`, response.status)
  }
  return data as T
}

const get = <T>(path: string) => request<T>(path)
const post = <T>(path: string, body?: unknown) =>
  request<T>(path, { method: 'POST', body: body === undefined ? undefined : JSON.stringify(body) })
const patch = <T>(path: string, body: unknown) =>
  request<T>(path, { method: 'PATCH', body: JSON.stringify(body) })
const put = <T>(path: string, body?: unknown) =>
  request<T>(path, { method: 'PUT', body: body === undefined ? undefined : JSON.stringify(body) })
const del = <T>(path: string) => request<T>(path, { method: 'DELETE' })

export interface SetupPayload {
  setup_token?: string
  username: string
  password: string
  display_name?: string
  email?: string
  instance_name: string
  tagline?: string
  description?: string
  icon_url?: string | null
  banner_url?: string | null
  accent_color?: string
  rules?: string
  welcome_message?: string
  registration_mode?: string
  require_rules_accept?: boolean
  default_role_name?: string
  default_permissions?: number
  roles?: { name: string; color?: string; permissions: number; hoist?: boolean }[]
  channels?: { name: string; kind: string; topic?: string; category?: string }[]
}

export const api = {
  meta: () => get<InstanceMeta>('/meta'),
  setup: (payload: SetupPayload) => post<{ token: string; user_id: string }>('/setup', payload),

  login: (username: string, password: string) =>
    post<{ token: string; user_id: string }>('/auth/login', { username, password }),
  register: (payload: {
    username: string
    password: string
    display_name?: string
    email?: string
    invite?: string
    accept_rules?: boolean
  }) => post<{ token: string; user_id: string }>('/auth/register', payload),
  me: () => get<Me>('/auth/me'),
  changePassword: (current_password: string, new_password: string) =>
    post<{ token: string }>('/auth/password', { current_password, new_password }),
  revokeSessions: () => post<{ token: string }>('/auth/revoke-sessions'),

  updateMe: (payload: Partial<Me> & { accept_rules?: boolean }) => patch<Me>('/users/@me', payload),
  user: (id: string) => get<Member>(`/users/${id}`),
  members: () => get<Member[]>('/members'),

  channels: () => get<Channel[]>('/channels'),
  channelPermissions: () => get<Record<string, string>>('/channels/permissions'),
  createChannel: (payload: Partial<Channel> & { name: string }) => post<Channel>('/channels', payload),
  updateChannel: (id: string, payload: Record<string, unknown>) =>
    patch<Channel>(`/channels/${id}`, payload),
  deleteChannel: (id: string) => del<{ ok: boolean }>(`/channels/${id}`),
  reorderChannels: (channels: { id: string; position: number; category_id: string | null }[]) =>
    post<{ ok: boolean }>('/channels/reorder', { channels }),
  overwrites: (id: string) => get<{ role_id: string; allow: string; deny: string }[]>(`/channels/${id}/overwrites`),
  setOverwrite: (id: string, role_id: string, allow: number, deny: number) =>
    put<{ ok: boolean }>(`/channels/${id}/overwrites`, { role_id, allow, deny }),

  categories: () => get<Category[]>('/categories'),
  createCategory: (name: string) => post<Category>('/categories', { name }),
  updateCategory: (id: string, name: string, position: number) =>
    patch<Category>(`/categories/${id}`, { name, position }),
  deleteCategory: (id: string) => del<{ ok: boolean }>(`/categories/${id}`),

  messages: (channelId: string, params: { before?: string; after?: string; limit?: number } = {}) => {
    const query = new URLSearchParams()
    if (params.before) query.set('before', params.before)
    if (params.after) query.set('after', params.after)
    if (params.limit) query.set('limit', String(params.limit))
    const suffix = query.toString() ? `?${query}` : ''
    return get<Message[]>(`/channels/${channelId}/messages${suffix}`)
  },
  sendMessage: (
    channelId: string,
    payload: { content: string; reply_to_id?: string | null; attachments?: unknown[] },
  ) => post<Message>(`/channels/${channelId}/messages`, payload),
  editMessage: (id: string, content: string) => patch<Message>(`/messages/${id}`, { content }),
  deleteMessage: (id: string) => del<{ ok: boolean }>(`/messages/${id}`),
  pinMessage: (id: string) => put<Message>(`/messages/${id}/pin`),
  unpinMessage: (id: string) => del<Message>(`/messages/${id}/pin`),
  pins: (channelId: string) => get<Message[]>(`/channels/${channelId}/pins`),
  addReaction: (id: string, emoji: string) =>
    put<{ ok: boolean }>(`/messages/${id}/reactions/${encodeURIComponent(emoji)}`),
  removeReaction: (id: string, emoji: string) =>
    del<{ ok: boolean }>(`/messages/${id}/reactions/${encodeURIComponent(emoji)}`),
  ack: (channelId: string, messageId: string) =>
    post<{ ok: boolean }>(`/channels/${channelId}/ack`, { message_id: messageId }),
  search: (params: { q: string; channel_id?: string; author_id?: string }) => {
    const query = new URLSearchParams({ q: params.q })
    if (params.channel_id) query.set('channel_id', params.channel_id)
    if (params.author_id) query.set('author_id', params.author_id)
    return get<Message[]>(`/search?${query}`)
  },

  upload: async (file: File, onProgress?: (percent: number) => void) => {
    const form = new FormData()
    form.append('file', file)
    // XHR rather than fetch: uploads need progress events for the UI.
    return new Promise<Attachment>((resolve, reject) => {
      const xhr = new XMLHttpRequest()
      xhr.open('POST', '/api/uploads')
      const token = getToken()
      if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`)
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable && onProgress) {
          onProgress(Math.round((event.loaded / event.total) * 100))
        }
      }
      xhr.onload = () => {
        try {
          const data = JSON.parse(xhr.responseText)
          if (xhr.status >= 200 && xhr.status < 300) resolve(data)
          else reject(new ApiError(data?.error ?? 'Upload failed', xhr.status))
        } catch {
          reject(new ApiError('Upload failed', xhr.status))
        }
      }
      xhr.onerror = () => reject(new ApiError('Upload failed. Check your connection.', 0))
      xhr.send(form)
    })
  },

  voiceToken: (channelId: string) =>
    post<{
      token: string
      url: string
      room: string
      can_speak: boolean
      can_video: boolean
      can_screen_share: boolean
    }>(`/voice/${channelId}/token`),
  voiceStates: () => get<VoiceState[]>('/voice/states'),
  forceDisconnect: (userId: string) => post<{ ok: boolean }>(`/voice/disconnect/${userId}`),

  invites: () => get<Invite[]>('/invites'),
  createInvite: (payload: {
    note?: string
    role_id?: string | null
    max_uses?: number
    expires_in_hours?: number
  }) => post<Invite>('/invites', payload),
  revokeInvite: (code: string) => del<{ ok: boolean }>(`/invites/${code}`),
  invitePreview: (code: string) => get<InvitePreview>(`/invites/preview/${code}`),

  adminInstance: () => get<Instance & { public_url: string; voice_enabled: boolean; livekit_url: string }>('/admin/instance'),
  updateInstance: (payload: Record<string, unknown>) => patch<Instance>('/admin/instance', payload),
  stats: () => get<Stats>('/admin/stats'),
  permissionCatalog: () => get<PermissionDef[]>('/admin/permissions'),
  roles: () => get<Role[]>('/admin/roles'),
  createRole: (payload: Record<string, unknown>) => post<Role>('/admin/roles', payload),
  updateRole: (id: string, payload: Record<string, unknown>) => patch<Role>(`/admin/roles/${id}`, payload),
  deleteRole: (id: string) => del<{ ok: boolean }>(`/admin/roles/${id}`),
  addMemberRole: (userId: string, roleId: string) =>
    put<{ ok: boolean }>(`/admin/members/${userId}/roles/${roleId}`),
  removeMemberRole: (userId: string, roleId: string) =>
    del<{ ok: boolean }>(`/admin/members/${userId}/roles/${roleId}`),
  updateMember: (userId: string, payload: Record<string, unknown>) =>
    patch<{ ok: boolean }>(`/admin/members/${userId}`, payload),
  kickMember: (userId: string) => del<{ ok: boolean }>(`/admin/members/${userId}`),
  bans: () => get<BanEntry[]>('/admin/bans'),
  banMember: (userId: string, reason: string) => put<{ ok: boolean }>(`/admin/bans/${userId}`, { reason }),
  unbanMember: (userId: string) => del<{ ok: boolean }>(`/admin/bans/${userId}`),
  audit: (params: { before?: string; action?: string } = {}) => {
    const query = new URLSearchParams()
    if (params.before) query.set('before', params.before)
    if (params.action) query.set('action', params.action)
    const suffix = query.toString() ? `?${query}` : ''
    return get<AuditEntry[]>(`/admin/audit${suffix}`)
  },
  webhooks: () => get<Webhook[]>('/admin/webhooks'),
  createWebhook: (payload: { channel_id: string; name: string; avatar_url?: string | null }) =>
    post<Webhook>('/admin/webhooks', payload),
  deleteWebhook: (id: string) => del<{ ok: boolean }>(`/admin/webhooks/${id}`),
}
