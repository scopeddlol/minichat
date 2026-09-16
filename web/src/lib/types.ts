export type Presence = 'online' | 'idle' | 'dnd' | 'offline'
export type ChannelKind = 'text' | 'voice' | 'announcement'
export type RegistrationMode = 'invite' | 'open' | 'closed'

export interface Member {
  id: string
  username: string
  display_name: string
  avatar_url: string | null
  banner_url: string | null
  bio: string
  pronouns: string
  favorite_game: string
  accent_color: string
  custom_status: string
  presence: Presence
  is_operator: boolean
  is_suspended: boolean
  created_at: string
  last_seen_at: string
  roles: string[]
  message_count?: number
}

export interface Me extends Member {
  email: string | null
  accepted_rules: boolean
  permissions: string
}

export interface Role {
  id: string
  name: string
  color: string | null
  permissions: number
  position: number
  is_default: boolean
  hoist: boolean
  mentionable: boolean
}

export interface Category {
  id: string
  name: string
  position: number
}

export interface Channel {
  id: string
  category_id: string | null
  kind: ChannelKind
  name: string
  topic: string
  position: number
  slowmode: number
  is_private: boolean
  user_limit: number
  created_at: string
}

export interface Attachment {
  id: string
  filename: string
  content_type: string
  size: number
  width: number | null
  height: number | null
  url: string
}

export interface ReactionGroup {
  emoji: string
  count: number
  me: boolean
}

export interface MessageAuthor {
  id: string
  username: string
  display_name: string
  avatar_url: string | null
  accent_color: string
  is_operator: boolean
  roles: string[]
}

export interface Message {
  id: string
  channel_id: string
  author: MessageAuthor | null
  content: string
  reply_to_id: string | null
  system_kind: string | null
  webhook_name: string | null
  pinned: boolean
  created_at: string
  edited_at: string | null
  attachments: Attachment[]
  reactions: ReactionGroup[]
  /** Set locally while a message is in flight. */
  pending?: boolean
  failed?: boolean
}

export interface Instance {
  name: string
  tagline: string
  description: string
  icon_url: string | null
  banner_url: string | null
  accent_color: string
  rules: string
  welcome_message: string
  setup_complete: boolean
  registration_mode: RegistrationMode
  require_rules_accept: boolean
  default_role_id: string | null
  system_channel_id: string | null
  max_upload_mb: number
  created_at: string
}

export interface InstanceMeta {
  name: string
  tagline: string
  description: string
  icon_url: string | null
  banner_url: string | null
  accent_color: string
  rules: string
  welcome_message: string
  setup_complete: boolean
  registration_mode: RegistrationMode
  require_rules_accept: boolean
  member_count: number
  voice_enabled: boolean
  setup_token_required: boolean
  version: string
}

export interface VoiceState {
  user_id: string
  channel_id: string
  muted: boolean
  deafened: boolean
  streaming: boolean
  video: boolean
}

export interface Invite {
  code: string
  created_by: string | null
  role_id: string | null
  note: string
  max_uses: number
  uses: number
  expires_at: string | null
  revoked: boolean
  created_at: string
  url: string
}

export interface Webhook {
  id: string
  channel_id: string
  name: string
  token: string
  avatar_url: string | null
  created_at: string
  url: string
}

export interface AuditEntry {
  id: string
  actor_id: string | null
  actor_name: string | null
  action: string
  target_type: string
  target_id: string
  detail: string
  created_at: string
}

export interface BanEntry {
  user_id: string
  username: string
  display_name: string
  reason: string
  banned_by: string | null
  created_at: string
}

export interface PermissionDef {
  key: string
  bit: string
  label: string
  group: 'general' | 'text' | 'voice' | 'admin'
}

export interface Stats {
  members: number
  online: number
  in_voice: number
  messages: number
  channels: number
  roles: number
  invites: number
  bans: number
  storage_bytes: number
  joined_last_week: number
  messages_last_week: number
  activity: { day: string; count: number }[]
  top_channels: { id: string; name: string; count: number }[]
  voice_enabled: boolean
  public_url: string
  version: string
}

export interface InvitePreview {
  valid: boolean
  reason: string
  inviter: string | null
  instance_name: string
  tagline: string
  description: string
  icon_url: string | null
  banner_url: string | null
  accent_color: string
  rules: string
  require_rules_accept: boolean
  member_count: number
}

export interface ReadyPayload {
  me: Me
  instance: Instance
  roles: Role[]
  categories: Category[]
  channels: Channel[]
  members: Member[]
  voice_states: VoiceState[]
  read_state: { channel_id: string; last_read_id: string }[]
  unread: Record<string, number>
  permissions: string
  channel_permissions: Record<string, string>
  voice_enabled: boolean
  livekit_url: string
}
