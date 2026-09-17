import {
  AlertCircle, CornerUpLeft, Download, Pencil, Pin, PinOff, RotateCw, SmilePlus, Trash2, Webhook,
} from 'lucide-react'
import { memo, useState } from 'react'
import { api } from '../lib/api'
import {
  formatBytes, formatTime, formatTimestamp, isAudio, isImage, isVideo,
} from '../lib/format'
import { isJumboEmoji, renderMarkdown } from '../lib/markdown'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import type { Attachment, Message } from '../lib/types'
import { Avatar, Badge, RoleFlair, toast, useConfirm } from './ui'

const QUICK_REACTIONS = ['👍', '❤️', '😂', '🎉', '👀', '🔥']

interface Props {
  message: Message
  grouped: boolean
  channelPermissions: bigint
  onReply: (message: Message) => void
  onOpenProfile: (userId: string) => void
  onOpenImage: (attachment: Attachment) => void
  highlight?: boolean
}

function MessageItemInner({
  message,
  grouped,
  channelPermissions,
  onReply,
  onOpenProfile,
  onOpenImage,
  highlight,
}: Props) {
  const me = useStore((s) => s.me)
  const members = useStore((s) => s.members)
  const roles = useStore((s) => s.roles)
  const emojis = useStore((s) => s.emojis)
  const messages = useStore((s) => s.messages)
  const retryMessage = useStore((s) => s.retryMessage)
  const jumpToMessage = useStore((s) => s.jumpToMessage)
  const confirm = useConfirm()

  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(message.content)
  const [pickerOpen, setPickerOpen] = useState(false)

  const isMine = message.author?.id === me?.id
  const canDelete = isMine || can(channelPermissions, P.MANAGE_MESSAGES)
  const canPin = can(channelPermissions, P.PIN_MESSAGES)
  const canReact = can(channelPermissions, P.ADD_REACTIONS)

  const author = message.author
  const displayName = message.webhook_name ?? author?.display_name ?? 'Deleted member'
  const roleColor = author
    ? roles
        .filter((role) => author.roles.includes(role.id) && role.color)
        .sort((a, b) => b.position - a.position)[0]?.color
    : null

  const replyTarget = message.reply_to_id
    ? (messages[message.channel_id] ?? []).find((m) => m.id === message.reply_to_id)
    : null

  if (message.system_kind === 'member_join') {
    return (
      <div className="flex items-center gap-2.5 px-4 py-1 text-sm" style={{ color: 'var(--text-muted)' }}>
        <span className="w-9 flex justify-center shrink-0">
          <span className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--success)' }} />
        </span>
        <span>
          <strong style={{ color: 'var(--text)' }}>{displayName}</strong> joined the instance.
        </span>
        <span className="text-xs" style={{ color: 'var(--text-faint)' }}>
          {formatTime(message.created_at)}
        </span>
      </div>
    )
  }

  const saveEdit = async () => {
    const trimmed = draft.trim()
    if (!trimmed || trimmed === message.content) {
      setEditing(false)
      setDraft(message.content)
      return
    }
    try {
      await api.editMessage(message.id, trimmed)
      setEditing(false)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Could not save your edit.')
    }
  }

  const remove = async () => {
    const ok = await confirm({
      title: 'Delete this message?',
      body: 'This cannot be undone.',
      confirmLabel: 'Delete',
      danger: true,
    })
    if (!ok) return
    try {
      await api.deleteMessage(message.id)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Could not delete the message.')
    }
  }

  const toggleReaction = async (emoji: string) => {
    const existing = message.reactions.find((r) => r.emoji === emoji)
    try {
      if (existing?.me) await api.removeReaction(message.id, emoji)
      else await api.addReaction(message.id, emoji)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Could not react.')
    }
    setPickerOpen(false)
  }

  const jumbo = isJumboEmoji(message.content) && !message.attachments.length

  return (
    <article
      className="group relative px-4 transition-colors"
      style={{
        background: highlight
          ? 'color-mix(in oklab, var(--accent) 10%, transparent)'
          : message.pinned
            ? 'color-mix(in oklab, var(--warning) 6%, transparent)'
            : undefined,
        paddingTop: grouped ? 1 : 8,
        paddingBottom: 1,
        opacity: message.pending ? 0.6 : 1,
      }}
      onMouseLeave={() => setPickerOpen(false)}
    >
      <div className="hidden group-hover:block absolute inset-0 pointer-events-none" style={{ background: 'var(--surface-1)', opacity: 0.55 }} />

      <div className="relative">
        {replyTarget && (
          <button
            className="flex items-center gap-1.5 mb-0.5 ml-12 text-[0.78rem] min-w-0 w-full text-left hover:opacity-80 transition-opacity"
            style={{ color: 'var(--text-faint)' }}
            onClick={() => jumpToMessage(message.channel_id, replyTarget.id)}
            title="Jump to the original message"
          >
            <CornerUpLeft size={12} className="shrink-0 -scale-y-100" />
            <Avatar
              id={replyTarget.author?.id ?? 'x'}
              name={replyTarget.author?.display_name ?? '?'}
              src={replyTarget.author?.avatar_url}
              accent={replyTarget.author?.accent_color}
              size="xs"
              className="!w-4 !h-4"
            />
            <span className="font-medium shrink-0" style={{ color: 'var(--text-muted)' }}>
              {replyTarget.author?.display_name ?? 'Deleted member'}
            </span>
            <span className="truncate">{replyTarget.content || 'Attachment'}</span>
          </button>
        )}

        <div className="flex gap-3">
          <div className="w-9 shrink-0 flex justify-center">
            {grouped ? (
              <span
                className="text-[0.65rem] opacity-0 group-hover:opacity-100 transition-opacity leading-6 tabular-nums"
                style={{ color: 'var(--text-faint)' }}
              >
                {formatTime(message.created_at)}
              </span>
            ) : message.webhook_name ? (
              <div
                className="w-9 h-9 rounded-full flex items-center justify-center"
                style={{ background: 'var(--surface-3)', color: 'var(--text-muted)' }}
              >
                <Webhook size={16} />
              </div>
            ) : (
              <button onClick={() => author && onOpenProfile(author.id)} className="rounded-full">
                <Avatar
                  id={author?.id ?? 'deleted'}
                  name={displayName}
                  src={author?.avatar_url}
                  accent={author?.accent_color}
                  size="md"
                />
              </button>
            )}
          </div>

          <div className="min-w-0 flex-1 pb-0.5">
            {!grouped && (
              <div className="flex items-baseline gap-2 flex-wrap">
                <button
                  className="font-semibold text-[0.92rem] hover:underline"
                  style={{ color: roleColor ?? 'var(--text)' }}
                  onClick={() => author && onOpenProfile(author.id)}
                >
                  {displayName}
                </button>
                {author && <RoleFlair roles={roles} memberRoleIds={author.roles} />}
                {message.webhook_name && <Badge>App</Badge>}
                {author?.is_operator && <Badge color="var(--accent)">Operator</Badge>}
                <time
                  className="text-[0.7rem]"
                  style={{ color: 'var(--text-faint)' }}
                  title={formatTimestamp(message.created_at)}
                >
                  {formatTimestamp(message.created_at)}
                </time>
                {message.pinned && (
                  <span className="inline-flex items-center gap-1 text-[0.68rem]" style={{ color: 'var(--warning)' }}>
                    <Pin size={10} /> Pinned
                  </span>
                )}
              </div>
            )}

            {editing ? (
              <div className="mt-1">
                <textarea
                  className="input resize-none"
                  rows={Math.min(8, draft.split('\n').length + 1)}
                  value={draft}
                  autoFocus
                  onChange={(event) => setDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey) {
                      event.preventDefault()
                      void saveEdit()
                    }
                    if (event.key === 'Escape') {
                      setEditing(false)
                      setDraft(message.content)
                    }
                  }}
                />
                <p className="text-xs mt-1" style={{ color: 'var(--text-faint)' }}>
                  Enter to save · Escape to cancel
                </p>
              </div>
            ) : (
              message.content && (
                <div
                  className="md text-[0.92rem] leading-[1.5] break-words whitespace-pre-wrap"
                  style={{ fontSize: jumbo ? '2.4rem' : undefined, lineHeight: jumbo ? 1.2 : undefined }}
                >
                  {renderMarkdown(message.content, {
                    members,
                    roles,
                    emojis,
                    meId: me?.id,
                    onMention: onOpenProfile,
                  })}
                  {message.edited_at && !jumbo && (
                    <span className="text-[0.66rem] ml-1.5" style={{ color: 'var(--text-faint)' }} title="Edited">
                      (edited)
                    </span>
                  )}
                </div>
              )
            )}

            {message.attachments.length > 0 && (
              <div className="mt-1.5 flex flex-col gap-1.5 items-start">
                {message.attachments.map((attachment) => (
                  <AttachmentView key={attachment.id} attachment={attachment} onOpenImage={onOpenImage} />
                ))}
              </div>
            )}

            {message.failed && (
              <div className="flex items-center gap-2 mt-1 text-[0.8rem]" style={{ color: 'var(--danger)' }}>
                <AlertCircle size={13} />
                <span>Failed to send.</span>
                <button
                  className="inline-flex items-center gap-1 font-medium hover:underline"
                  onClick={() => void retryMessage(message.channel_id, message.id)}
                >
                  <RotateCw size={12} /> Retry
                </button>
              </div>
            )}

            {message.reactions.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-1.5">
                {message.reactions.map((reaction) => (
                  <button
                    key={reaction.emoji}
                    onClick={() => canReact && void toggleReaction(reaction.emoji)}
                    disabled={!canReact}
                    className="flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[0.78rem] border transition-colors"
                    style={{
                      background: reaction.me ? 'var(--accent-soft)' : 'var(--surface-2)',
                      borderColor: reaction.me ? 'var(--accent)' : 'transparent',
                    }}
                  >
                    <ReactionGlyph emoji={reaction.emoji} emojis={emojis} />
                    <span className="tabular-nums" style={{ color: reaction.me ? 'var(--accent)' : 'var(--text-muted)' }}>
                      {reaction.count}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Hover toolbar */}
        {!editing && !message.pending && (
          <div
            className="absolute -top-3.5 right-2 hidden group-hover:flex items-center rounded-lg border overflow-hidden shadow-sm"
            style={{ background: 'var(--surface-2)' }}
          >
            {pickerOpen && (
              <div className="flex items-center border-r" style={{ borderColor: 'var(--border)' }}>
                {emojis.slice(0, 4).map((emoji) => (
                  <button
                    key={emoji.id}
                    title={`:${emoji.name}:`}
                    className="px-1.5 py-1.5 transition-transform hover:scale-125"
                    onClick={() => void toggleReaction(`:${emoji.name}:`)}
                  >
                    <img src={emoji.url} alt={emoji.name} className="w-4 h-4 object-contain" />
                  </button>
                ))}
                {QUICK_REACTIONS.map((emoji) => (
                  <button
                    key={emoji}
                    className="px-1.5 py-1.5 text-sm transition-transform hover:scale-125"
                    onClick={() => void toggleReaction(emoji)}
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            )}
            {canReact && (
              <ToolbarButton title="Add reaction" onClick={() => setPickerOpen((value) => !value)}>
                <SmilePlus size={14} />
              </ToolbarButton>
            )}
            <ToolbarButton title="Reply" onClick={() => onReply(message)}>
              <CornerUpLeft size={14} />
            </ToolbarButton>
            {isMine && !message.webhook_name && (
              <ToolbarButton
                title="Edit"
                onClick={() => {
                  setDraft(message.content)
                  setEditing(true)
                }}
              >
                <Pencil size={14} />
              </ToolbarButton>
            )}
            {canPin && (
              <ToolbarButton
                title={message.pinned ? 'Unpin' : 'Pin'}
                onClick={async () => {
                  try {
                    if (message.pinned) await api.unpinMessage(message.id)
                    else await api.pinMessage(message.id)
                  } catch (error) {
                    toast.error(error instanceof Error ? error.message : 'Could not pin.')
                  }
                }}
              >
                {message.pinned ? <PinOff size={14} /> : <Pin size={14} />}
              </ToolbarButton>
            )}
            {canDelete && (
              <ToolbarButton title="Delete" onClick={remove} danger>
                <Trash2 size={14} />
              </ToolbarButton>
            )}
          </div>
        )}
      </div>
    </article>
  )
}

/** A reaction is either a unicode emoji or a `:name:` custom-emoji reference. */
function ReactionGlyph({ emoji, emojis }: { emoji: string; emojis: import('../lib/types').Emoji[] }) {
  if (emoji.startsWith(':') && emoji.endsWith(':')) {
    const name = emoji.slice(1, -1)
    const custom = emojis.find((e) => e.name === name)
    if (custom) {
      return <img src={custom.url} alt={emoji} title={emoji} className="reaction-emoji" />
    }
    // The emoji was deleted after someone reacted with it.
    return <span title={emoji}>❔</span>
  }
  return <span>{emoji}</span>
}

function ToolbarButton({
  children,
  title,
  onClick,
  danger,
}: {
  children: React.ReactNode
  title: string
  onClick: () => void
  danger?: boolean
}) {
  return (
    <button
      title={title}
      aria-label={title}
      onClick={onClick}
      className="px-2 py-1.5 transition-colors hover:bg-[var(--surface-3)]"
      style={{ color: danger ? 'var(--danger)' : 'var(--text-muted)' }}
    >
      {children}
    </button>
  )
}

function AttachmentView({
  attachment,
  onOpenImage,
}: {
  attachment: Attachment
  onOpenImage: (attachment: Attachment) => void
}) {
  if (isImage(attachment.content_type)) {
    // Reserve the right box before the image loads so the list doesn't jump.
    const ratio = attachment.width && attachment.height ? attachment.width / attachment.height : 16 / 9
    const width = Math.min(attachment.width ?? 400, 400)
    return (
      <button
        onClick={() => onOpenImage(attachment)}
        className="rounded-xl overflow-hidden border max-w-full transition-opacity hover:opacity-92"
        style={{ width, aspectRatio: ratio }}
      >
        <img
          src={attachment.url}
          alt={attachment.filename}
          loading="lazy"
          className="w-full h-full object-cover"
        />
      </button>
    )
  }

  if (isVideo(attachment.content_type)) {
    return (
      <video
        src={attachment.url}
        controls
        preload="metadata"
        className="rounded-xl border max-w-full"
        style={{ maxWidth: 440 }}
      />
    )
  }

  if (isAudio(attachment.content_type)) {
    return (
      <div className="rounded-xl border p-2.5 w-full" style={{ background: 'var(--surface-2)', maxWidth: 380 }}>
        <p className="text-xs mb-1.5 truncate font-medium">{attachment.filename}</p>
        <audio src={attachment.url} controls className="w-full" style={{ height: 32 }} />
      </div>
    )
  }

  return (
    <a
      href={attachment.url}
      download={attachment.filename}
      className="flex items-center gap-2.5 px-3 py-2.5 rounded-xl border transition-colors hover:border-[var(--accent)]"
      style={{ background: 'var(--surface-2)', maxWidth: 340 }}
    >
      <div
        className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
        style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
      >
        <Download size={15} />
      </div>
      <div className="min-w-0">
        <p className="text-[0.82rem] font-medium truncate">{attachment.filename}</p>
        <p className="text-[0.7rem]" style={{ color: 'var(--text-faint)' }}>
          {formatBytes(attachment.size)}
        </p>
      </div>
    </a>
  )
}

export default memo(MessageItemInner)
