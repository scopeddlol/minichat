import { ArrowDown, Hash, Loader2, Megaphone, Sparkles } from 'lucide-react'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { formatDayDivider, parseDate, shouldGroup } from '../lib/format'
import { useStore } from '../lib/store'
import type { Attachment, Channel, Message } from '../lib/types'
import MessageItem from './MessageItem'
import { EmptyState, Spinner } from './ui'

interface Props {
  channel: Channel
  channelPermissions: bigint
  onReply: (message: Message) => void
  onOpenProfile: (userId: string) => void
  onOpenImage: (attachment: Attachment) => void
}

export default function MessageList({
  channel,
  channelPermissions,
  onReply,
  onOpenProfile,
  onOpenImage,
}: Props) {
  const messages = useStore((s) => s.messages[channel.id])
  const loading = useStore((s) => s.loadingChannel[channel.id])
  const hasMore = useStore((s) => s.hasMore[channel.id])
  const loadOlder = useStore((s) => s.loadOlder)
  const markRead = useStore((s) => s.markRead)
  const instance = useStore((s) => s.instance)

  const scroller = useRef<HTMLDivElement>(null)
  const bottomAnchor = useRef<HTMLDivElement>(null)
  const [atBottom, setAtBottom] = useState(true)
  const pinnedToBottom = useRef(true)
  const previousHeight = useRef(0)
  const previousChannel = useRef(channel.id)
  const previousCount = useRef(0)

  const list = messages ?? []

  const scrollToBottom = useCallback((behavior: ScrollBehavior = 'auto') => {
    bottomAnchor.current?.scrollIntoView({ block: 'end', behavior })
  }, [])

  const onScroll = useCallback(() => {
    const element = scroller.current
    if (!element) return
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight
    const bottom = distance < 80
    pinnedToBottom.current = bottom
    setAtBottom(bottom)
    if (bottom) markRead(channel.id)

    // Fetch the previous page as the top comes into view.
    if (element.scrollTop < 240 && hasMore && !loading) {
      previousHeight.current = element.scrollHeight
      void loadOlder(channel.id)
    }
  }, [channel.id, hasMore, loading, loadOlder, markRead])

  // Jump to the newest message when the channel changes.
  useLayoutEffect(() => {
    if (previousChannel.current !== channel.id) {
      previousChannel.current = channel.id
      pinnedToBottom.current = true
      previousCount.current = list.length
      requestAnimationFrame(() => scrollToBottom())
      return
    }

    const element = scroller.current
    if (!element) return

    // Preserve the reading position after prepending older messages.
    if (previousHeight.current && element.scrollHeight > previousHeight.current) {
      const grew = element.scrollHeight - previousHeight.current
      const prepended = list.length > previousCount.current && !pinnedToBottom.current
      if (prepended) element.scrollTop += grew
      previousHeight.current = 0
    } else if (pinnedToBottom.current && list.length !== previousCount.current) {
      scrollToBottom()
    }
    previousCount.current = list.length
  }, [channel.id, list.length, scrollToBottom])

  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === 'visible' && pinnedToBottom.current) markRead(channel.id)
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => document.removeEventListener('visibilitychange', onVisible)
  }, [channel.id, markRead])

  if (loading && !list.length) {
    return (
      <div className="flex-1 flex items-center justify-center" style={{ color: 'var(--text-faint)' }}>
        <Spinner size={22} />
      </div>
    )
  }

  return (
    <div className="flex-1 relative min-h-0">
      <div ref={scroller} onScroll={onScroll} className="h-full overflow-y-auto scroll-thin overscroll-contain">
        <div className="min-h-full flex flex-col">
        <div className="mt-auto" />
        {hasMore && (
          <div className="flex justify-center py-3">
            {loading ? (
              <Loader2 size={16} className="animate-spin" style={{ color: 'var(--text-faint)' }} />
            ) : (
              <button
                className="btn btn-ghost !text-xs"
                onClick={() => {
                  previousHeight.current = scroller.current?.scrollHeight ?? 0
                  void loadOlder(channel.id)
                }}
              >
                Load earlier messages
              </button>
            )}
          </div>
        )}

        {!hasMore && <ChannelIntro channel={channel} welcome={instance?.welcome_message ?? ''} />}

        {list.map((message, index) => {
          const previous = list[index - 1]
          const showDivider =
            !previous ||
            parseDate(previous.created_at).toDateString() !== parseDate(message.created_at).toDateString()

          return (
            <div key={message.id}>
              {showDivider && <DayDivider label={formatDayDivider(message.created_at)} />}
              <MessageItem
                message={message}
                grouped={!showDivider && shouldGroup(previous, message)}
                channelPermissions={channelPermissions}
                onReply={onReply}
                onOpenProfile={onOpenProfile}
                onOpenImage={onOpenImage}
              />
            </div>
          )
        })}

        {!list.length && !loading && (
          <EmptyState
            icon={<Sparkles size={22} />}
            title="No messages yet"
            body={`Say something in #${channel.name} to get things started.`}
          />
        )}

        <div ref={bottomAnchor} className="h-4" />
        </div>
      </div>

      {!atBottom && (
        <button
          onClick={() => scrollToBottom('smooth')}
          className="absolute bottom-4 right-4 btn btn-subtle !rounded-full !px-3 animate-fade-up shadow-lg"
          style={{ boxShadow: 'var(--shadow-md)' }}
        >
          <ArrowDown size={15} /> Jump to latest
        </button>
      )}
    </div>
  )
}

function DayDivider({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-3 px-4 my-3.5">
      <div className="flex-1 h-px" style={{ background: 'var(--border)' }} />
      <span className="text-[0.68rem] font-semibold uppercase tracking-wider" style={{ color: 'var(--text-faint)' }}>
        {label}
      </span>
      <div className="flex-1 h-px" style={{ background: 'var(--border)' }} />
    </div>
  )
}

function ChannelIntro({ channel, welcome }: { channel: Channel; welcome: string }) {
  return (
    <div className="px-4 pt-8 pb-5">
      <div
        className="w-14 h-14 rounded-2xl flex items-center justify-center mb-3.5"
        style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
      >
        {channel.kind === 'announcement' ? <Megaphone size={26} /> : <Hash size={26} />}
      </div>
      <h2 className="text-2xl font-semibold">Welcome to #{channel.name}</h2>
      <p className="text-sm mt-1.5 max-w-lg leading-relaxed" style={{ color: 'var(--text-muted)' }}>
        {channel.topic
          ? channel.topic
          : channel.kind === 'announcement'
            ? 'This is an announcement channel — only members with permission can post here.'
            : `This is the start of #${channel.name}.`}
      </p>
      {welcome && (
        <p
          className="text-sm mt-3 px-3.5 py-2.5 rounded-xl max-w-lg"
          style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
        >
          {welcome}
        </p>
      )}
    </div>
  )
}
