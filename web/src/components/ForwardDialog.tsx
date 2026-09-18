import { CornerUpRight, Hash } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { ApiError } from '../lib/api'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import type { Message } from '../lib/types'
import { Field, Modal, toast } from './ui'

/**
 * Send someone else's message to another channel.
 *
 * Forwarded as a quote with attribution and a link back to the original,
 * rather than as a copy under your own name: the point of a forward is
 * "look what they said over there", and dropping the attribution turns it
 * into you saying it.
 *
 * Only channels you can actually post in are offered, and the target's
 * permissions are checked by the server on send regardless.
 */
export default function ForwardDialog({
  open,
  onClose,
  message,
}: {
  open: boolean
  onClose: () => void
  message: Message | null
}) {
  const channels = useStore((s) => s.channels)
  const channelPermissions = useStore((s) => s.channelPermissions)
  const permissions = useStore((s) => s.permissions)
  const sendMessage = useStore((s) => s.sendMessage)
  const setActiveChannel = useStore((s) => s.setActiveChannel)

  const [target, setTarget] = useState('')
  const [note, setNote] = useState('')
  const [busy, setBusy] = useState(false)

  const sources = useMemo(
    () =>
      channels.filter(
        (channel) =>
          channel.kind !== 'voice' &&
          can(channelPermissions[channel.id] ?? permissions, P.SEND_MESSAGES),
      ),
    [channels, channelPermissions, permissions],
  )

  useEffect(() => {
    if (!open) return
    setNote('')
    setTarget(sources.find((channel) => channel.id !== message?.channel_id)?.id ?? '')
  }, [open, sources, message])

  const origin = channels.find((channel) => channel.id === message?.channel_id)

  const send = async () => {
    if (!message || !target) return
    setBusy(true)
    try {
      const author = message.webhook_name ?? message.author?.display_name ?? 'someone'
      const where = origin ? ` in #${origin.name}` : ''
      // Every line quoted, so a multi-line message stays visually one block
      // instead of only its first line reading as a quote.
      const quoted = message.content
        .split('\n')
        .map((line) => `> ${line}`)
        .join('\n')
      const link = `${location.origin}/?channel=${message.channel_id}&message=${message.id}`
      // Bare URLs, not `[text](url)`: this renderer autolinks the former and
      // shows the latter as literal text (see the `link` rule in markdown.tsx).
      const parts = [
        `**${author}**${where}:`,
        quoted,
        // Attachments don't travel with the text, so at least link them.
        ...message.attachments.map((file) => file.url),
        note.trim(),
        link,
      ].filter(Boolean)

      await sendMessage(target, parts.join('\n'), null, [])
      toast.success('Forwarded.')
      setActiveChannel(target)
      onClose()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not forward that message.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Forward message"
      description="Sent as a quote, with a link back to the original."
      width="sm"
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={send} disabled={busy || !target}>
            <CornerUpRight size={14} />
            {busy ? 'Sending…' : 'Forward'}
          </button>
        </>
      }
    >
      <div className="px-5 py-4 space-y-4">
        {message && (
          <div
            className="rounded-xl p-3 text-sm max-h-32 overflow-y-auto scroll-thin"
            style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
          >
            <p className="text-xs font-semibold mb-1" style={{ color: 'var(--text)' }}>
              {message.webhook_name ?? message.author?.display_name ?? 'Deleted member'}
            </p>
            <p className="whitespace-pre-wrap break-words">
              {message.content || <em>No text — attachments only</em>}
            </p>
          </div>
        )}

        <Field label="Send to">
          {sources.length === 0 ? (
            <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
              There's nowhere you can post right now.
            </p>
          ) : (
            <div className="space-y-1 max-h-48 overflow-y-auto scroll-thin">
              {sources.map((channel) => (
                <button
                  key={channel.id}
                  onClick={() => setTarget(channel.id)}
                  className="w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm transition-colors"
                  style={{
                    background: target === channel.id ? 'var(--accent-soft)' : 'transparent',
                    color: target === channel.id ? 'var(--accent)' : 'var(--text-muted)',
                  }}
                >
                  <span className="shrink-0">{channel.emoji || <Hash size={14} />}</span>
                  <span className="truncate">{channel.name}</span>
                  {channel.id === message?.channel_id && (
                    <span className="text-[0.68rem] ml-auto" style={{ color: 'var(--text-faint)' }}>
                      here
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}
        </Field>

        <Field label="Add a note" hint="Optional">
          <input
            className="input"
            value={note}
            maxLength={500}
            onChange={(event) => setNote(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && void send()}
            placeholder="Thought you'd want to see this"
          />
        </Field>
      </div>
    </Modal>
  )
}
