import type { ReactNode } from 'react'
import { Fragment, useState } from 'react'
import type { Emoji, Member, Role } from './types'

/**
 * A small markdown subset rendered straight to React nodes. Nothing is ever
 * inserted as HTML, so a message can't inject markup no matter what it says.
 */

interface RenderContext {
  members: Record<string, Member>
  roles: Role[]
  emojis: Emoji[]
  /** Highlights mentions of the signed-in member. */
  meId?: string
  onMention?: (userId: string) => void
}

function Spoiler({ children }: { children: ReactNode }) {
  const [revealed, setRevealed] = useState(false)
  return (
    <span
      className={`spoiler${revealed ? ' revealed' : ''}`}
      onClick={(event) => {
        event.stopPropagation()
        setRevealed((value) => !value)
      }}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') setRevealed((value) => !value)
      }}
    >
      {children}
    </span>
  )
}

// Ordered by precedence: the first match at a position wins.
const INLINE_RULES: {
  name: string
  pattern: RegExp
}[] = [
  { name: 'code', pattern: /`([^`\n]+)`/ },
  { name: 'bold', pattern: /\*\*([\s\S]+?)\*\*/ },
  { name: 'underline', pattern: /__([\s\S]+?)__/ },
  { name: 'strike', pattern: /~~([\s\S]+?)~~/ },
  { name: 'italic', pattern: /(?:\*([^*\n]+?)\*|_([^_\n]+?)_)/ },
  { name: 'spoiler', pattern: /\|\|([\s\S]+?)\|\|/ },
  { name: 'link', pattern: /(https?:\/\/[^\s<>()]+[^\s<>().,!?;:'"])/ },
  // A mention must start at a word boundary so an email address doesn't
  // become a ping. Mirrors the server's parser in `server/src/mentions.rs`.
  { name: 'mention', pattern: /(?<![A-Za-z0-9_.\-])@([A-Za-z0-9_][A-Za-z0-9_.-]{1,31})/ },
  { name: 'emoji', pattern: /:([a-z0-9_]{2,32}):/i },
]

function renderInline(text: string, context: RenderContext, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = []
  let remaining = text
  let key = 0

  while (remaining.length > 0) {
    let bestIndex = -1
    let bestRule: (typeof INLINE_RULES)[number] | null = null
    let bestMatch: RegExpMatchArray | null = null

    for (const rule of INLINE_RULES) {
      const match = remaining.match(rule.pattern)
      if (match && match.index !== undefined && (bestIndex === -1 || match.index < bestIndex)) {
        bestIndex = match.index
        bestRule = rule
        bestMatch = match
      }
    }

    if (!bestRule || !bestMatch || bestIndex === -1) {
      nodes.push(remaining)
      break
    }

    if (bestIndex > 0) nodes.push(remaining.slice(0, bestIndex))
    const raw = bestMatch[0]
    const inner = bestMatch[1] ?? bestMatch[2] ?? ''
    const id = `${keyPrefix}-${key++}`

    switch (bestRule.name) {
      case 'code':
        nodes.push(<code key={id}>{inner}</code>)
        break
      case 'bold':
        nodes.push(<strong key={id}>{renderInline(inner, context, id)}</strong>)
        break
      case 'underline':
        nodes.push(<u key={id}>{renderInline(inner, context, id)}</u>)
        break
      case 'strike':
        nodes.push(<s key={id}>{renderInline(inner, context, id)}</s>)
        break
      case 'italic':
        nodes.push(<em key={id}>{renderInline(inner, context, id)}</em>)
        break
      case 'spoiler':
        nodes.push(<Spoiler key={id}>{renderInline(inner, context, id)}</Spoiler>)
        break
      case 'link':
        nodes.push(
          <a key={id} href={raw} target="_blank" rel="noopener noreferrer nofollow">
            {raw.length > 64 ? `${raw.slice(0, 61)}…` : raw}
          </a>,
        )
        break
      case 'mention': {
        // Trailing punctuation isn't part of a username: "@ada." → "ada".
        let name = inner
        let trailing = ''
        while (name.endsWith('.') || name.endsWith('-')) {
          trailing = name.slice(-1) + trailing
          name = name.slice(0, -1)
        }

        if (name.toLowerCase() === 'everyone' || name.toLowerCase() === 'here') {
          nodes.push(
            <span key={id} className="mention mention-self">
              @{name}
            </span>,
          )
          if (trailing) nodes.push(trailing)
          break
        }

        const member = Object.values(context.members).find(
          (m) => m.username.toLowerCase() === name.toLowerCase(),
        )
        if (!member) {
          // Not a real member — leave it as ordinary text.
          nodes.push(raw)
          break
        }
        nodes.push(
          <span
            key={id}
            className={`mention${member.id === context.meId ? ' mention-self' : ''}`}
            onClick={() => context.onMention?.(member.id)}
            role="button"
            tabIndex={0}
          >
            @{member.display_name}
          </span>,
        )
        if (trailing) nodes.push(trailing)
        break
      }
      case 'emoji': {
        const emoji = context.emojis.find((e) => e.name === inner.toLowerCase())
        if (!emoji) {
          nodes.push(raw)
          break
        }
        nodes.push(
          <img
            key={id}
            src={emoji.url}
            alt={`:${emoji.name}:`}
            title={`:${emoji.name}:`}
            className="custom-emoji"
            loading="lazy"
          />,
        )
        break
      }
    }

    remaining = remaining.slice(bestIndex + raw.length)
  }

  return nodes
}

export function renderMarkdown(content: string, context: RenderContext): ReactNode {
  if (!content) return null

  // Fenced code blocks are extracted first so nothing inside them is parsed.
  const segments = content.split(/```(?:([a-zA-Z0-9+#.-]*)\n)?([\s\S]*?)```/g)
  const output: ReactNode[] = []

  for (let i = 0; i < segments.length; i += 3) {
    const plain = segments[i]
    if (plain) output.push(<Fragment key={`t${i}`}>{renderBlocks(plain, context, `t${i}`)}</Fragment>)

    const code = segments[i + 2]
    if (code !== undefined) {
      output.push(
        <pre key={`c${i}`}>
          <code>{code.replace(/\n$/, '')}</code>
        </pre>,
      )
    }
  }

  return output
}

function renderBlocks(text: string, context: RenderContext, keyPrefix: string): ReactNode[] {
  const lines = text.split('\n')
  const blocks: ReactNode[] = []
  let quote: string[] = []

  const flushQuote = (index: number) => {
    if (!quote.length) return
    blocks.push(
      <blockquote key={`${keyPrefix}-q${index}`}>
        {quote.map((line, i) => (
          <Fragment key={i}>
            {i > 0 && <br />}
            {renderInline(line, context, `${keyPrefix}-q${index}-${i}`)}
          </Fragment>
        ))}
      </blockquote>,
    )
    quote = []
  }

  lines.forEach((line, index) => {
    const quoted = line.match(/^>\s?(.*)$/)
    if (quoted) {
      quote.push(quoted[1])
      return
    }
    flushQuote(index)
    blocks.push(
      <Fragment key={`${keyPrefix}-l${index}`}>
        {index > 0 && blocks.length > 0 && <br />}
        {renderInline(line, context, `${keyPrefix}-l${index}`)}
      </Fragment>,
    )
  })
  flushQuote(lines.length)

  return blocks
}

/** Emoji-only messages render larger, the way chat apps usually do. */
export function isJumboEmoji(content: string): boolean {
  const trimmed = content.trim()
  if (!trimmed || trimmed.length > 24) return false
  const withoutEmoji = trimmed.replace(
    /[\p{Extended_Pictographic}\p{Emoji_Component}‍️\s]/gu,
    '',
  )
  return withoutEmoji.length === 0
}
