import type { Channel } from './types'

/** Insert before a channel, or append to a category when target is null. */
export function moveChannel(
  channels: Channel[],
  id: string,
  category: string | null,
  target: string | null,
) {
  const moving = channels.find((c) => c.id === id)
  if (!moving || target === id) return channels
  const remaining = channels.filter((c) => c.id !== id)
  let index = target ? remaining.findIndex((c) => c.id === target) : -1
  if (index < 0) {
    index = remaining.reduce((last, c, i) => (c.category_id === category ? i + 1 : last), 0)
    if (index === 0) index = remaining.length
  }
  remaining.splice(index, 0, { ...moving, category_id: category })
  return remaining.map((c, position) => ({ ...c, position }))
}
