import { Activity, HardDrive, Hash, MessageSquare, Radio, Shield, Users, Zap } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api } from '../../lib/api'
import { formatBytes } from '../../lib/format'
import type { Stats } from '../../lib/types'
import { Spinner, copyText } from '../ui'

export default function Overview() {
  const [stats, setStats] = useState<Stats | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    api.stats().then(setStats).catch((e) => setError(e.message))
  }, [])

  if (error) return <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>
  if (!stats)
    return (
      <div className="flex justify-center py-12" style={{ color: 'var(--text-faint)' }}>
        <Spinner size={22} />
      </div>
    )

  // Zero-fill the window so a quiet instance shows a real 14-day series
  // rather than one bar stretched across the whole chart.
  const series = buildSeries(stats.activity, 14)
  const peak = Math.max(1, ...series.map((point) => point.count))

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <StatCard icon={<Users size={16} />} label="Members" value={stats.members} sub={`+${stats.joined_last_week} this week`} />
        <StatCard icon={<Radio size={16} />} label="Online now" value={stats.online} sub={`${stats.in_voice} in voice`} accent />
        <StatCard
          icon={<MessageSquare size={16} />}
          label="Messages"
          value={stats.messages}
          sub={`+${stats.messages_last_week} this week`}
        />
        <StatCard icon={<HardDrive size={16} />} label="Uploads" value={formatBytes(stats.storage_bytes)} sub="total stored" />
      </div>

      <section className="card p-5">
        <h3 className="font-semibold text-sm flex items-center gap-2 mb-4">
          <Activity size={15} style={{ color: 'var(--accent)' }} /> Messages per day
        </h3>
        {stats.messages > 0 ? (
          <div className="flex items-end gap-1.5" style={{ height: 120 }}>
            {series.map((point) => (
              <div key={point.day} className="flex-1 flex flex-col items-center gap-1.5 group min-w-0">
                <span
                  className="text-[0.62rem] opacity-0 group-hover:opacity-100 transition-opacity tabular-nums"
                  style={{ color: 'var(--text-muted)' }}
                >
                  {point.count}
                </span>
                <div
                  className="w-full rounded-t transition-all"
                  style={{
                    height: `${Math.max(3, (point.count / peak) * 88)}px`,
                    background: point.count ? 'var(--accent)' : 'var(--surface-3)',
                    opacity: point.count ? 0.7 : 1,
                  }}
                />
                <span className="text-[0.58rem] truncate w-full text-center" style={{ color: 'var(--text-faint)' }}>
                  {point.day.slice(5).replace('-', '/')}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm py-6 text-center" style={{ color: 'var(--text-faint)' }}>
            No messages in the last two weeks.
          </p>
        )}
      </section>

      <div className="grid md:grid-cols-2 gap-4">
        <section className="card p-5">
          <h3 className="font-semibold text-sm flex items-center gap-2 mb-3">
            <Hash size={15} style={{ color: 'var(--accent)' }} /> Busiest channels
          </h3>
          <div className="space-y-2">
            {stats.top_channels.map((channel) => (
              <div key={channel.id} className="flex items-center gap-3">
                <span className="text-sm truncate flex-1 min-w-0">#{channel.name}</span>
                <div className="w-24 h-1.5 rounded-full overflow-hidden shrink-0" style={{ background: 'var(--surface-3)' }}>
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${(channel.count / Math.max(1, stats.top_channels[0].count)) * 100}%`,
                      background: 'var(--accent)',
                    }}
                  />
                </div>
                <span className="text-xs tabular-nums w-10 text-right" style={{ color: 'var(--text-faint)' }}>
                  {channel.count}
                </span>
              </div>
            ))}
            {!stats.top_channels.length && (
              <p className="text-sm" style={{ color: 'var(--text-faint)' }}>
                No channels yet.
              </p>
            )}
          </div>
        </section>

        <section className="card p-5">
          <h3 className="font-semibold text-sm flex items-center gap-2 mb-3">
            <Zap size={15} style={{ color: 'var(--accent)' }} /> Instance
          </h3>
          <dl className="space-y-2.5 text-sm">
            <Row label="Public URL">
              <button className="hover:underline truncate" style={{ color: 'var(--accent)' }} onClick={() => copyText(stats.public_url)}>
                {stats.public_url}
              </button>
            </Row>
            <Row label="Voice &amp; video">
              <span style={{ color: stats.voice_enabled ? 'var(--success)' : 'var(--warning)' }}>
                {stats.voice_enabled ? 'Enabled' : 'Not configured'}
              </span>
            </Row>
            <Row label="Channels">{stats.channels}</Row>
            <Row label="Roles">{stats.roles}</Row>
            <Row label="Active invites">{stats.invites}</Row>
            <Row label="Bans">{stats.bans}</Row>
            <Row label="Version">v{stats.version}</Row>
          </dl>
          {!stats.voice_enabled && (
            <p
              className="text-xs mt-4 p-3 rounded-lg leading-relaxed"
              style={{ background: 'color-mix(in oklab, var(--warning) 10%, transparent)', color: 'var(--warning)' }}
            >
              <Shield size={12} className="inline mr-1" />
              Set LIVEKIT_URL, LIVEKIT_API_KEY and LIVEKIT_API_SECRET in your .env, then restart. HTTPS is
              required for browsers to grant microphone and camera access.
            </p>
          )}
        </section>
      </div>
    </div>
  )
}

/** Pads a sparse day/count list into a contiguous window ending today. */
function buildSeries(activity: { day: string; count: number }[], days: number) {
  const counts = new Map(activity.map((point) => [point.day, point.count]))
  const series: { day: string; count: number }[] = []
  const today = new Date()
  for (let offset = days - 1; offset >= 0; offset--) {
    const date = new Date(today)
    date.setDate(today.getDate() - offset)
    const key = date.toISOString().slice(0, 10)
    series.push({ day: key, count: counts.get(key) ?? 0 })
  }
  return series
}

function StatCard({
  icon,
  label,
  value,
  sub,
  accent,
}: {
  icon: React.ReactNode
  label: string
  value: number | string
  sub: string
  accent?: boolean
}) {
  return (
    <div className="card p-4">
      <div className="flex items-center gap-2 mb-2" style={{ color: accent ? 'var(--accent)' : 'var(--text-faint)' }}>
        {icon}
        <span className="text-[0.68rem] font-bold uppercase tracking-wide">{label}</span>
      </div>
      <p className="text-2xl font-semibold tabular-nums leading-none">{value}</p>
      <p className="text-[0.7rem] mt-1.5" style={{ color: 'var(--text-faint)' }}>
        {sub}
      </p>
    </div>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <dt style={{ color: 'var(--text-muted)' }}>{label}</dt>
      <dd className="font-medium truncate min-w-0 text-right">{children}</dd>
    </div>
  )
}
