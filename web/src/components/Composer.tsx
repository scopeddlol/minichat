import { AtSign, CornerUpLeft, Loader2, Plus, SendHorizontal, Smile, X } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../lib/api'
import { formatBytes, formatSlowmode, isImage } from '../lib/format'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import type { Attachment, Channel, Emoji, Member, Message } from '../lib/types'
import { Avatar, toast } from './ui'

/** One row in the autocomplete popup. */
interface Suggestion {
  key: string
  /** Text substituted into the draft when chosen. */
  insert: string
  member?: Member
  emoji?: Emoji
}

/** A live `@name` or `:name` token sitting just behind the caret. */
interface ActiveToken {
  kind: 'mention' | 'emoji'
  query: string
  /** Offset of the token's opening character in the draft. */
  start: number
}

/**
 * Find the token the caret is currently inside, if any. Returns null when the
 * caret isn't in one, which closes the popup.
 */
function tokenAtCaret(text: string, caret: number): ActiveToken | null {
  for (let i = caret - 1; i >= 0 && caret - i <= 33; i--) {
    const c = text[i]
    if (c === '@' || c === ':') {
      // Must start at a word boundary, so emails and `a:b` don't trigger it.
      const previous = i > 0 ? text[i - 1] : ' '
      if (/[A-Za-z0-9_.\-]/.test(previous)) return null
      const query = text.slice(i + 1, caret)
      if (!/^[A-Za-z0-9_.\-]*$/.test(query)) return null
      if (c === ':' && query.length < 1) return null
      return { kind: c === '@' ? 'mention' : 'emoji', query, start: i }
    }
    if (!/[A-Za-z0-9_.\-]/.test(c)) return null
  }
  return null
}

const EMOJI = [
  '😀','😂','🥹','😊','😍','🤔','😎','🙃','😴','🤯','🥳','😭','😤','🤝','👍','👎','👏','🙌',
  '🔥','✨','🎉','❤️','💜','💚','👀','🚀','💡','✅','❌','⚠️','📌','🎮','🎧','☕','🍕','🌙',
]

interface Props {
  channel: Channel
  channelPermissions: bigint
  replyTo: Message | null
  onCancelReply: () => void
}

interface PendingUpload {
  id: string
  file: File
  progress: number
  attachment: Attachment | null
  error: string
}

export default function Composer({ channel, channelPermissions, replyTo, onCancelReply }: Props) {
  const sendMessage = useStore((s) => s.sendMessage)
  const sendTyping = useStore((s) => s.sendTyping)
  const instance = useStore((s) => s.instance)

  const [draft, setDraft] = useState('')
  const [uploads, setUploads] = useState<PendingUpload[]>([])
  const [sending, setSending] = useState(false)
  const [emojiOpen, setEmojiOpen] = useState(false)
  const [cooldown, setCooldown] = useState(0)
  const [dragging, setDragging] = useState(false)
  const [token, setToken] = useState<ActiveToken | null>(null)
  const [highlighted, setHighlighted] = useState(0)

  const textarea = useRef<HTMLTextAreaElement>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const lastTyping = useRef(0)

  const members = useStore((s) => s.members)
  const emojis = useStore((s) => s.emojis)
  const me = useStore((s) => s.me)

  const canSend = can(channelPermissions, P.SEND_MESSAGES)
  const canAttach = can(channelPermissions, P.ATTACH_FILES)
  const bypassSlowmode = can(channelPermissions, P.MANAGE_MESSAGES)
  const canMentionEveryone = can(channelPermissions, P.MENTION_EVERYONE)

  // Draft per channel, so switching away doesn't lose what you typed.
  useEffect(() => {
    setDraft(sessionStorage.getItem(`minichat.draft.${channel.id}`) ?? '')
    setUploads([])
    setCooldown(0)
  }, [channel.id])

  useEffect(() => {
    const id = setTimeout(() => {
      try {
        if (draft) sessionStorage.setItem(`minichat.draft.${channel.id}`, draft)
        else sessionStorage.removeItem(`minichat.draft.${channel.id}`)
      } catch {
        /* storage unavailable */
      }
    }, 400)
    return () => clearTimeout(id)
  }, [draft, channel.id])

  useEffect(() => {
    if (replyTo) textarea.current?.focus()
  }, [replyTo])

  useEffect(() => {
    if (cooldown <= 0) return
    const id = setInterval(() => setCooldown((value) => Math.max(0, value - 1)), 1000)
    return () => clearInterval(id)
  }, [cooldown])

  const suggestions = useMemo<Suggestion[]>(() => {
    if (!token) return []
    const query = token.query.toLowerCase()

    if (token.kind === 'mention') {
      const matches = Object.values(members)
        .filter(
          (member) =>
            member.id !== me?.id &&
            (member.username.toLowerCase().startsWith(query) ||
              member.display_name.toLowerCase().includes(query)),
        )
        // Prefer a username prefix match over a display-name substring.
        .sort((a, b) => {
          const aPrefix = a.username.toLowerCase().startsWith(query) ? 0 : 1
          const bPrefix = b.username.toLowerCase().startsWith(query) ? 0 : 1
          return aPrefix - bPrefix || a.display_name.localeCompare(b.display_name)
        })
        .slice(0, 8)
        .map<Suggestion>((member) => ({
          key: member.id,
          insert: `@${member.username}`,
          member,
        }))

      if (canMentionEveryone && 'everyone'.startsWith(query)) {
        matches.unshift({ key: 'everyone', insert: '@everyone' })
      }
      return matches
    }

    return emojis
      .filter((emoji) => emoji.name.includes(query))
      .slice(0, 8)
      .map<Suggestion>((emoji) => ({ key: emoji.id, insert: `:${emoji.name}:`, emoji }))
  }, [token, members, emojis, me?.id, canMentionEveryone])

  useEffect(() => setHighlighted(0), [token?.query, token?.kind])

  const applySuggestion = useCallback(
    (index: number) => {
      const choice = suggestions[index]
      if (!choice || !token) return
      const element = textarea.current
      const caret = element?.selectionStart ?? draft.length
      const next = `${draft.slice(0, token.start)}${choice.insert} ${draft.slice(caret)}`
      setDraft(next)
      setToken(null)
      requestAnimationFrame(() => {
        const position = token.start + choice.insert.length + 1
        element?.focus()
        element?.setSelectionRange(position, position)
      })
    },
    [suggestions, token, draft],
  )

  const autoGrow = () => {
    const element = textarea.current
    if (!element) return
    element.style.height = 'auto'
    element.style.height = `${Math.min(element.scrollHeight, 220)}px`
  }

  useEffect(autoGrow, [draft])

  const onChange = (value: string, caret: number) => {
    setDraft(value)
    setToken(tokenAtCaret(value, caret))
    const now = Date.now()
    // Throttle typing notifications; the server broadcasts every one it gets.
    if (now - lastTyping.current > 3000 && value.trim()) {
      lastTyping.current = now
      sendTyping(channel.id)
    }
  }

  const addFiles = async (files: FileList | File[]) => {
    if (!canAttach) {
      toast.error("You don't have permission to attach files here.")
      return
    }
    const limit = (instance?.max_upload_mb ?? 25) * 1024 * 1024
    for (const file of Array.from(files).slice(0, 10)) {
      if (file.size > limit) {
        toast.error(`${file.name} is larger than the ${instance?.max_upload_mb ?? 25} MB limit.`)
        continue
      }
      const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
      setUploads((value) => [...value, { id, file, progress: 0, attachment: null, error: '' }])
      try {
        const attachment = await api.upload(file, (progress) =>
          setUploads((value) => value.map((u) => (u.id === id ? { ...u, progress } : u))),
        )
        setUploads((value) => value.map((u) => (u.id === id ? { ...u, attachment, progress: 100 } : u)))
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Upload failed'
        setUploads((value) => value.map((u) => (u.id === id ? { ...u, error: message } : u)))
      }
    }
  }

  const submit = async () => {
    const content = draft.trim()
    const ready = uploads.filter((u) => u.attachment).map((u) => u.attachment as Attachment)
    if ((!content && !ready.length) || sending || cooldown > 0) return
    if (uploads.some((u) => !u.attachment && !u.error)) {
      toast.info('Still uploading — one moment.')
      return
    }

    setSending(true)
    try {
      await sendMessage(channel.id, content, replyTo, ready)
      setDraft('')
      setUploads([])
      onCancelReply()
      try {
        sessionStorage.removeItem(`minichat.draft.${channel.id}`)
      } catch {
        /* ignore */
      }
      if (channel.slowmode > 0 && !bypassSlowmode) setCooldown(channel.slowmode)
      requestAnimationFrame(autoGrow)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Could not send your message.')
    } finally {
      setSending(false)
      textarea.current?.focus()
    }
  }

  if (!canSend) {
    return (
      <div className="px-4 pb-4 pt-1 safe-bottom">
        <div
          className="rounded-xl px-4 py-3 text-sm text-center"
          style={{ background: 'var(--surface-1)', color: 'var(--text-faint)' }}
        >
          {channel.kind === 'announcement'
            ? 'Only members with permission can post in this announcement channel.'
            : "You don't have permission to send messages here."}
        </div>
      </div>
    )
  }

  return (
    <div className="px-3 sm:px-4 pb-3 pt-1 safe-bottom">
      {replyTo && (
        <div
          className="flex items-center gap-2 px-3 py-1.5 text-[0.8rem] rounded-t-xl border border-b-0 animate-fade-up"
          style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
        >
          <CornerUpLeft size={13} className="-scale-y-100 shrink-0" />
          <span className="truncate flex-1">
            Replying to <strong style={{ color: 'var(--text)' }}>{replyTo.author?.display_name ?? 'a message'}</strong>
          </span>
          <button className="btn btn-ghost !p-1" onClick={onCancelReply} aria-label="Cancel reply">
            <X size={13} />
          </button>
        </div>
      )}

      {token && suggestions.length > 0 && (
        <div
          className="card mb-1 p-1 max-h-60 overflow-y-auto scroll-thin animate-pop-in"
          style={{ boxShadow: 'var(--shadow-lg)' }}
        >
          <p
            className="text-[0.62rem] font-bold uppercase tracking-wider px-2 py-1"
            style={{ color: 'var(--text-faint)' }}
          >
            {token.kind === 'mention' ? 'Members' : 'Custom emoji'}
          </p>
          {suggestions.map((choice, index) => (
            <button
              key={choice.key}
              type="button"
              onMouseDown={(event) => {
                event.preventDefault()
                applySuggestion(index)
              }}
              onMouseEnter={() => setHighlighted(index)}
              className="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-left transition-colors"
              style={{
                background: index === highlighted ? 'var(--surface-2)' : 'transparent',
              }}
            >
              {choice.member ? (
                <>
                  <Avatar
                    id={choice.member.id}
                    name={choice.member.display_name}
                    src={choice.member.avatar_url}
                    frame={choice.member.avatar_frame}
                    accent={choice.member.accent_color}
                    size="sm"
                  />
                  <span className="text-sm font-medium truncate">{choice.member.display_name}</span>
                  <span className="text-xs truncate" style={{ color: 'var(--text-faint)' }}>
                    @{choice.member.username}
                  </span>
                </>
              ) : choice.emoji ? (
                <>
                  <img src={choice.emoji.url} alt="" className="w-6 h-6 object-contain" />
                  <span className="text-sm">:{choice.emoji.name}:</span>
                </>
              ) : (
                <>
                  <span
                    className="w-7 h-7 rounded-full flex items-center justify-center shrink-0"
                    style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
                  >
                    <AtSign size={14} />
                  </span>
                  <span className="text-sm font-medium">@everyone</span>
                  <span className="text-xs" style={{ color: 'var(--text-faint)' }}>
                    notifies everyone in this channel
                  </span>
                </>
              )}
            </button>
          ))}
        </div>
      )}

      <div
        className="rounded-xl border transition-colors relative"
        style={{
          background: 'var(--surface-1)',
          borderColor: dragging ? 'var(--accent)' : 'var(--border)',
          borderTopLeftRadius: replyTo ? 0 : undefined,
          borderTopRightRadius: replyTo ? 0 : undefined,
        }}
        onDragOver={(event) => {
          event.preventDefault()
          if (canAttach) setDragging(true)
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(event) => {
          event.preventDefault()
          setDragging(false)
          if (event.dataTransfer.files.length) void addFiles(event.dataTransfer.files)
        }}
      >
        {dragging && (
          <div
            className="absolute inset-0 z-10 rounded-xl flex items-center justify-center text-sm font-medium pointer-events-none"
            style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
          >
            Drop files to attach
          </div>
        )}

        {uploads.length > 0 && (
          <div className="flex gap-2 p-2.5 pb-0 flex-wrap">
            {uploads.map((upload) => (
              <div
                key={upload.id}
                className="relative rounded-lg border overflow-hidden group"
                style={{ background: 'var(--surface-2)', width: 108 }}
              >
                {isImage(upload.file.type) ? (
                  <img
                    src={URL.createObjectURL(upload.file)}
                    alt=""
                    className="w-full object-cover"
                    style={{ height: 68 }}
                    onLoad={(event) => URL.revokeObjectURL((event.target as HTMLImageElement).src)}
                  />
                ) : (
                  <div className="flex items-center justify-center" style={{ height: 68, color: 'var(--text-faint)' }}>
                    <Plus size={18} />
                  </div>
                )}
                <div className="px-1.5 py-1">
                  <p className="text-[0.65rem] truncate">{upload.file.name}</p>
                  <p className="text-[0.6rem]" style={{ color: upload.error ? 'var(--danger)' : 'var(--text-faint)' }}>
                    {upload.error
                      ? 'Failed'
                      : upload.attachment
                        ? formatBytes(upload.file.size)
                        : `${upload.progress}%`}
                  </p>
                </div>
                {!upload.attachment && !upload.error && (
                  <div className="absolute inset-x-0 bottom-0 h-0.5" style={{ background: 'var(--surface-3)' }}>
                    <div className="h-full transition-all" style={{ width: `${upload.progress}%`, background: 'var(--accent)' }} />
                  </div>
                )}
                <button
                  className="absolute top-1 right-1 rounded-md p-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
                  style={{ background: 'var(--surface-0)' }}
                  onClick={() => setUploads((value) => value.filter((u) => u.id !== upload.id))}
                  aria-label="Remove attachment"
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="flex items-end gap-1 p-1.5">
          {canAttach && (
            <>
              <button
                className="btn btn-ghost !p-2 shrink-0"
                onClick={() => fileInput.current?.click()}
                title="Attach a file"
                aria-label="Attach a file"
              >
                <Plus size={18} />
              </button>
              <input
                ref={fileInput}
                type="file"
                multiple
                className="hidden"
                onChange={(event) => {
                  if (event.target.files?.length) void addFiles(event.target.files)
                  event.target.value = ''
                }}
              />
            </>
          )}

          <textarea
            ref={textarea}
            rows={1}
            value={draft}
            onChange={(event) => onChange(event.target.value, event.target.selectionStart ?? 0)}
            onClick={(event) =>
              setToken(
                tokenAtCaret(
                  (event.target as HTMLTextAreaElement).value,
                  (event.target as HTMLTextAreaElement).selectionStart ?? 0,
                ),
              )
            }
            onBlur={() => setTimeout(() => setToken(null), 150)}
            onPaste={(event) => {
              const files = Array.from(event.clipboardData.files)
              if (files.length && canAttach) {
                event.preventDefault()
                void addFiles(files)
              }
            }}
            onKeyDown={(event) => {
              // While the autocomplete is open it owns the arrow keys, Enter
              // and Tab; otherwise Enter sends as usual.
              if (token && suggestions.length) {
                if (event.key === 'ArrowDown') {
                  event.preventDefault()
                  setHighlighted((value) => (value + 1) % suggestions.length)
                  return
                }
                if (event.key === 'ArrowUp') {
                  event.preventDefault()
                  setHighlighted((value) => (value - 1 + suggestions.length) % suggestions.length)
                  return
                }
                if (event.key === 'Enter' || event.key === 'Tab') {
                  event.preventDefault()
                  applySuggestion(highlighted)
                  return
                }
                if (event.key === 'Escape') {
                  event.preventDefault()
                  setToken(null)
                  return
                }
              }
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                void submit()
              }
            }}
            placeholder={
              cooldown > 0
                ? `Slow mode — ${cooldown}s`
                : channel.slowmode > 0
                  ? `Message #${channel.name} (slow mode: ${formatSlowmode(channel.slowmode)})`
                  : `Message #${channel.name}`
            }
            disabled={cooldown > 0}
            className="flex-1 bg-transparent resize-none outline-none py-2 px-1 text-[0.92rem] leading-relaxed min-w-0 scroll-thin disabled:opacity-60"
            style={{ maxHeight: 220 }}
          />

          <div className="relative shrink-0">
            <button
              className="btn btn-ghost !p-2"
              onClick={() => setEmojiOpen((value) => !value)}
              title="Emoji"
              aria-label="Insert emoji"
            >
              <Smile size={18} />
            </button>
            {emojiOpen && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setEmojiOpen(false)} />
                <div
                  className="absolute bottom-full right-0 mb-2 z-20 card p-2 animate-pop-in max-h-72 overflow-y-auto scroll-thin"
                  style={{ width: 296, boxShadow: 'var(--shadow-lg)' }}
                >
                  {emojis.length > 0 && (
                    <>
                      <p
                        className="text-[0.62rem] font-bold uppercase tracking-wider px-1 pb-1"
                        style={{ color: 'var(--text-faint)' }}
                      >
                        This instance
                      </p>
                      <div className="grid grid-cols-9 gap-0.5 mb-2">
                        {emojis.map((emoji) => (
                          <button
                            key={emoji.id}
                            title={`:${emoji.name}:`}
                            className="p-1 rounded transition-transform hover:scale-125"
                            onClick={() => {
                              setDraft((value) => `${value}:${emoji.name}: `)
                              setEmojiOpen(false)
                              textarea.current?.focus()
                            }}
                          >
                            <img src={emoji.url} alt={emoji.name} className="w-6 h-6 object-contain" />
                          </button>
                        ))}
                      </div>
                      <p
                        className="text-[0.62rem] font-bold uppercase tracking-wider px-1 pb-1"
                        style={{ color: 'var(--text-faint)' }}
                      >
                        Standard
                      </p>
                    </>
                  )}
                  <div className="grid grid-cols-9 gap-0.5">
                    {EMOJI.map((emoji) => (
                      <button
                        key={emoji}
                        className="p-1 rounded text-lg transition-transform hover:scale-125"
                        onClick={() => {
                          setDraft((value) => value + emoji)
                          setEmojiOpen(false)
                          textarea.current?.focus()
                        }}
                      >
                        {emoji}
                      </button>
                    ))}
                  </div>
                </div>
              </>
            )}
          </div>

          <button
            className="btn btn-primary !p-2 shrink-0"
            onClick={() => void submit()}
            disabled={sending || cooldown > 0 || (!draft.trim() && !uploads.some((u) => u.attachment))}
            title="Send"
            aria-label="Send message"
          >
            {sending ? <Loader2 size={18} className="animate-spin" /> : <SendHorizontal size={18} />}
          </button>
        </div>
      </div>

      <TypingIndicator channelId={channel.id} />
    </div>
  )
}

function TypingIndicator({ channelId }: { channelId: string }) {
  const typing = useStore((s) => s.typing[channelId]) ?? []
  if (!typing.length) return <div className="h-4" />

  const names = typing.map((entry) => entry.displayName)
  const label =
    names.length === 1
      ? `${names[0]} is typing`
      : names.length === 2
        ? `${names[0]} and ${names[1]} are typing`
        : `${names.length} people are typing`

  return (
    <div className="h-4 flex items-center gap-1.5 px-1 mt-0.5 text-[0.72rem] animate-fade-in" style={{ color: 'var(--text-muted)' }}>
      <span className="flex gap-0.5">
        {[0, 1, 2].map((index) => (
          <span
            key={index}
            className="w-1 h-1 rounded-full"
            style={{
              background: 'var(--text-faint)',
              animation: `fade-in 0.6s ${index * 0.15}s infinite alternate`,
            }}
          />
        ))}
      </span>
      {label}…
    </div>
  )
}
