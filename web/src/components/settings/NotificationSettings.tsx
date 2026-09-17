import { Bell, BellOff, Loader2, Send, Smartphone } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import * as push from '../../lib/push'
import { useStore } from '../../lib/store'
import type { NotificationMode } from '../../lib/types'
import Select from '../Select'
import { ChannelIcon } from '../Sidebar'
import { Field, toast } from '../ui'

const MODES: { value: NotificationMode; label: string; hint: string }[] = [
  { value: 'all', label: 'Every message', hint: 'Notify me about everything.' },
  { value: 'mentions', label: 'Only @mentions', hint: 'Just when someone names me.' },
  { value: 'none', label: 'Nothing', hint: 'Never notify me.' },
]

export default function NotificationSettings() {
  const channels = useStore((s) => s.channels)
  const preferences = useStore((s) => s.notifications)
  const setNotifications = useStore((s) => s.setNotifications)
  const pushEnabled = useStore((s) => s.pushEnabled)

  const [subscribed, setSubscribed] = useState(false)
  const [busy, setBusy] = useState(false)
  const [checked, setChecked] = useState(false)

  useEffect(() => {
    push.isEnabled().then((value) => {
      setSubscribed(value)
      setChecked(true)
    })
  }, [])

  const supported = push.isSupported()
  const needsInstall = push.isIos() && !push.isStandalone()
  const blocked = push.permission() === 'denied'

  const toggleSubscription = async () => {
    setBusy(true)
    try {
      if (subscribed) {
        await push.disable()
        setSubscribed(false)
        toast.info('Notifications turned off on this device.')
      } else {
        const result = await push.enable()
        if (result.ok) {
          setSubscribed(true)
          toast.success('Notifications are on for this device.')
        } else {
          toast.error(result.reason ?? 'Could not enable notifications.')
        }
      }
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not change notifications.')
    } finally {
      setBusy(false)
    }
  }

  const setMode = async (mode: NotificationMode) => {
    try {
      setNotifications(await api.updateNotifications({ mode }))
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save that.')
    }
  }

  const setChannelMode = async (channelId: string, mode: NotificationMode | null) => {
    try {
      setNotifications(await api.updateNotifications({ channel_id: channelId, channel_mode: mode }))
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save that.')
    }
  }

  const overrideFor = (channelId: string) =>
    preferences.channels.find((entry) => entry.channel_id === channelId)?.mode ?? null

  return (
    <div className="space-y-6 animate-fade-in">
      <h2 className="text-lg font-semibold">Notifications</h2>

      {/* Device subscription */}
      <section className="card p-4">
        <div className="flex items-start gap-3">
          <div
            className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
            style={{
              background: subscribed ? 'var(--accent-soft)' : 'var(--surface-3)',
              color: subscribed ? 'var(--accent)' : 'var(--text-muted)',
            }}
          >
            {subscribed ? <Bell size={17} /> : <BellOff size={17} />}
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="font-semibold text-sm">Push notifications on this device</h3>

            {!pushEnabled ? (
              <p className="text-xs mt-1 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                The operator hasn't configured push for this instance yet. They need to set
                <code className="mx-1">VAPID_PUBLIC_KEY</code> and
                <code className="mx-1">VAPID_PRIVATE_KEY</code> in <code>.env</code>.
              </p>
            ) : !supported ? (
              <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
                This browser doesn't support push notifications.
              </p>
            ) : needsInstall ? (
              <p
                className="text-xs mt-2 p-2.5 rounded-lg leading-relaxed flex gap-2"
                style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
              >
                <Smartphone size={14} className="shrink-0 mt-0.5" />
                <span>
                  On iPhone and iPad, Safari only allows notifications for installed apps. Tap
                  Share → <strong>Add to Home Screen</strong>, then turn them on from there.
                </span>
              </p>
            ) : blocked ? (
              <p className="text-xs mt-1 leading-relaxed" style={{ color: 'var(--danger)' }}>
                Notifications are blocked for this site. Re-allow them in your browser's site
                settings, then come back.
              </p>
            ) : (
              <p className="text-xs mt-1 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                {subscribed
                  ? 'This device will receive notifications even when MiniChat is closed.'
                  : 'Get notified when someone messages you while MiniChat is closed.'}
              </p>
            )}

            {pushEnabled && supported && !needsInstall && !blocked && checked && (
              <div className="flex gap-2 mt-3">
                <button
                  className={`btn ${subscribed ? 'btn-subtle' : 'btn-primary'}`}
                  onClick={toggleSubscription}
                  disabled={busy}
                >
                  {busy ? <Loader2 size={14} className="animate-spin" /> : <Bell size={14} />}
                  {subscribed ? 'Turn off' : 'Turn on'}
                </button>
                {subscribed && (
                  <button
                    className="btn btn-ghost"
                    onClick={async () => {
                      try {
                        await api.pushTest()
                        toast.info('Test notification sent — check your device.')
                      } catch {
                        toast.error('Could not send a test notification.')
                      }
                    }}
                  >
                    <Send size={14} /> Send a test
                  </button>
                )}
              </div>
            )}
          </div>
        </div>
      </section>

      {/* Global default */}
      <section>
        <Field label="Notify me about" hint="The default for every channel you haven't customised.">
          <div className="grid sm:grid-cols-3 gap-2">
            {MODES.map((option) => (
              <button
                key={option.value}
                onClick={() => void setMode(option.value)}
                className="p-3 rounded-xl border text-left transition-colors"
                style={{
                  borderColor: preferences.mode === option.value ? 'var(--accent)' : 'var(--border)',
                  background: preferences.mode === option.value ? 'var(--accent-soft)' : 'transparent',
                }}
              >
                <span
                  className="text-sm font-medium block"
                  style={{ color: preferences.mode === option.value ? 'var(--accent)' : 'var(--text)' }}
                >
                  {option.label}
                </span>
                <span className="text-xs block mt-0.5" style={{ color: 'var(--text-muted)' }}>
                  {option.hint}
                </span>
              </button>
            ))}
          </div>
        </Field>
      </section>

      {/* Per-channel overrides */}
      <section>
        <span className="label">Per-channel</span>
        <div className="card divide-y overflow-hidden">
          {channels
            .filter((channel) => channel.kind !== 'voice')
            .map((channel) => {
              const override = overrideFor(channel.id)
              return (
                <div key={channel.id} className="flex items-center gap-2 px-3 py-2">
                  <ChannelIcon kind={channel.kind} isPrivate={channel.is_private} size={14} />
                  <span className="text-sm flex-1 min-w-0 truncate">{channel.name}</span>
                  <Select
                    width="auto"
                    className="!py-1 !text-xs"
                    ariaLabel={`Notifications for ${channel.name}`}
                    value={override ?? 'inherit'}
                    onChange={(next) =>
                      void setChannelMode(
                        channel.id,
                        next === 'inherit' ? null : (next as NotificationMode),
                      )
                    }
                    options={[
                      { value: 'inherit', label: 'Default' },
                      { value: 'all', label: 'Everything' },
                      { value: 'mentions', label: 'Mentions' },
                      { value: 'none', label: 'Muted' },
                    ]}
                  />
                </div>
              )
            })}
          {!channels.filter((c) => c.kind !== 'voice').length && (
            <p className="text-sm px-3 py-6 text-center" style={{ color: 'var(--text-faint)' }}>
              No channels yet.
            </p>
          )}
        </div>
      </section>
    </div>
  )
}
