/** SQLite hands back "YYYY-MM-DD HH:MM:SS" in UTC; make it a real Date. */
export function parseDate(value: string): Date {
  if (!value) return new Date()
  if (value.includes('T')) return new Date(value)
  return new Date(value.replace(' ', 'T') + 'Z')
}

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })
const dateFormat = new Intl.DateTimeFormat(undefined, {
  month: 'long',
  day: 'numeric',
  year: 'numeric',
})
const shortDateFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })

export function formatTime(value: string): string {
  return timeFormat.format(parseDate(value))
}

export function formatFullDate(value: string): string {
  return dateFormat.format(parseDate(value))
}

export function formatShortDate(value: string): string {
  return shortDateFormat.format(parseDate(value))
}

export function formatTimestamp(value: string): string {
  const date = parseDate(value)
  const now = new Date()
  const sameDay = date.toDateString() === now.toDateString()
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)

  if (sameDay) return `Today at ${timeFormat.format(date)}`
  if (date.toDateString() === yesterday.toDateString()) return `Yesterday at ${timeFormat.format(date)}`
  return `${dateFormat.format(date)} at ${timeFormat.format(date)}`
}

export function formatDayDivider(value: string): string {
  const date = parseDate(value)
  const now = new Date()
  if (date.toDateString() === now.toDateString()) return 'Today'
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (date.toDateString() === yesterday.toDateString()) return 'Yesterday'
  return dateFormat.format(date)
}

export function formatRelative(value: string): string {
  const seconds = Math.floor((Date.now() - parseDate(value).getTime()) / 1000)
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return shortDateFormat.format(parseDate(value))
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`
}

export function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const secs = Math.floor(seconds % 60)
  const pad = (n: number) => String(n).padStart(2, '0')
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(secs)}` : `${minutes}:${pad(secs)}`
}

export function formatSlowmode(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`
  return `${Math.round(seconds / 3600)}h`
}

/** Deterministic fallback avatar colour derived from a user ID. */
export function avatarGradient(id: string, accent: string): string {
  let hash = 0
  for (let i = 0; i < id.length; i++) hash = (hash * 31 + id.charCodeAt(i)) >>> 0
  return `color-mix(in oklab, ${accent} 12%, hsl(220 4% ${30 + hash % 9}%))`
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (!parts.length) return '?'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}

export function isImage(contentType: string): boolean {
  return contentType.startsWith('image/')
}

export function isVideo(contentType: string): boolean {
  return contentType.startsWith('video/')
}

export function isAudio(contentType: string): boolean {
  return contentType.startsWith('audio/')
}

/** True when two consecutive messages should render as one group. */
export function shouldGroup(previous: import('./types').Message | undefined, current: import('./types').Message): boolean {
  if (!previous) return false
  if (previous.system_kind || current.system_kind) return false
  if (previous.webhook_name !== current.webhook_name) return false
  if (previous.author?.id !== current.author?.id) return false
  if (current.reply_to_id) return false
  const gap = parseDate(current.created_at).getTime() - parseDate(previous.created_at).getTime()
  return gap < 5 * 60 * 1000
}
