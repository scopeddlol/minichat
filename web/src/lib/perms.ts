/**
 * Permission bits mirror `server/src/perms.rs`. They are handled as BigInt
 * because the flags run past the safe integer range once ADMINISTRATOR is set.
 */
export const P = {
  VIEW_CHANNELS: 1n << 0n,
  SEND_MESSAGES: 1n << 1n,
  MANAGE_MESSAGES: 1n << 2n,
  ATTACH_FILES: 1n << 3n,
  ADD_REACTIONS: 1n << 4n,
  MENTION_EVERYONE: 1n << 5n,
  CONNECT: 1n << 6n,
  SPEAK: 1n << 7n,
  VIDEO: 1n << 8n,
  SCREEN_SHARE: 1n << 9n,
  MUTE_MEMBERS: 1n << 10n,
  MOVE_MEMBERS: 1n << 11n,
  MANAGE_CHANNELS: 1n << 12n,
  MANAGE_ROLES: 1n << 13n,
  MANAGE_INSTANCE: 1n << 14n,
  CREATE_INVITES: 1n << 15n,
  KICK_MEMBERS: 1n << 16n,
  BAN_MEMBERS: 1n << 17n,
  VIEW_AUDIT_LOG: 1n << 18n,
  MANAGE_WEBHOOKS: 1n << 19n,
  MANAGE_NICKNAMES: 1n << 20n,
  PIN_MESSAGES: 1n << 21n,
  MANAGE_EMOJI: 1n << 22n,
  ADMINISTRATOR: 1n << 30n,
} as const

export type PermissionName = keyof typeof P

export function toBits(value: string | number | bigint | undefined | null): bigint {
  if (value === undefined || value === null) return 0n
  try {
    return BigInt(value)
  } catch {
    return 0n
  }
}

/** ADMINISTRATOR implies every other permission. */
export function can(bits: bigint, permission: bigint): boolean {
  return (bits & P.ADMINISTRATOR) !== 0n || (bits & permission) === permission
}

/** True when any of the listed permissions would open an admin surface. */
export const ADMIN_PERMS = [
  P.MANAGE_INSTANCE,
  P.MANAGE_CHANNELS,
  P.MANAGE_ROLES,
  P.KICK_MEMBERS,
  P.BAN_MEMBERS,
  P.VIEW_AUDIT_LOG,
  P.MANAGE_WEBHOOKS,
  P.MANAGE_EMOJI,
]

export function canSeeAdminPanel(bits: bigint): boolean {
  return ADMIN_PERMS.some((p) => can(bits, p))
}
