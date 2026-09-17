import { Copy, Plus, Trash2, Webhook as WebhookIcon } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { useStore } from '../../lib/store'
import type { Webhook } from '../../lib/types'
import Select from '../Select'
import { Field, Modal, copyText, toast, useConfirm } from '../ui'

export default function Integrations() {
  const channels = useStore((s) => s.channels)
  const confirm = useConfirm()

  const [hooks, setHooks] = useState<Webhook[]>([])
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [channelId, setChannelId] = useState('')
  const [busy, setBusy] = useState(false)

  const load = () => api.webhooks().then(setHooks).catch(() => undefined)

  useEffect(() => {
    void load()
  }, [])

  useEffect(() => {
    const textChannels = channels.filter((channel) => channel.kind !== 'voice')
    if (!channelId && textChannels.length) setChannelId(textChannels[0].id)
  }, [channels, channelId])

  const create = async () => {
    setBusy(true)
    try {
      await api.createWebhook({ channel_id: channelId, name: name.trim() })
      setName('')
      setCreating(false)
      void load()
      toast.success('Webhook created.')
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not create the webhook.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-4 animate-fade-in max-w-3xl">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="font-semibold">Incoming webhooks</h3>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            Let external services post into a channel — CI results, alerts, RSS, anything that can send
            an HTTP request.
          </p>
        </div>
        <button className="btn btn-primary shrink-0" onClick={() => setCreating(true)}>
          <Plus size={15} /> New webhook
        </button>
      </div>

      <div className="card divide-y overflow-hidden">
        {hooks.map((hook) => {
          const channel = channels.find((c) => c.id === hook.channel_id)
          return (
            <div key={hook.id} className="px-4 py-3">
              <div className="flex items-center gap-3">
                <div
                  className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
                  style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
                >
                  <WebhookIcon size={15} />
                </div>
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium truncate">{hook.name}</p>
                  <p className="text-xs" style={{ color: 'var(--text-faint)' }}>
                    posts to #{channel?.name ?? 'deleted channel'}
                  </p>
                </div>
                <button
                  className="btn btn-subtle !py-1.5 !px-2.5 shrink-0"
                  onClick={() => copyText(hook.url, 'Webhook URL copied')}
                  title="Copy webhook URL"
                >
                  <Copy size={14} />
                </button>
                <button
                  className="btn btn-ghost !p-1.5 shrink-0"
                  style={{ color: 'var(--danger)' }}
                  title="Delete webhook"
                  onClick={async () => {
                    const ok = await confirm({
                      title: `Delete "${hook.name}"?`,
                      body: 'Anything posting to this URL stops working immediately.',
                      confirmLabel: 'Delete',
                      danger: true,
                    })
                    if (!ok) return
                    await api.deleteWebhook(hook.id).catch(() => toast.error('Failed.'))
                    void load()
                  }}
                >
                  <Trash2 size={14} />
                </button>
              </div>

              <details className="mt-2.5">
                <summary className="text-xs cursor-pointer select-none" style={{ color: 'var(--accent)' }}>
                  Show usage
                </summary>
                <pre
                  className="mt-2 p-3 rounded-lg text-[0.7rem] overflow-x-auto scroll-thin"
                  style={{ background: 'var(--surface-0)', color: 'var(--text-muted)' }}
                >
{`curl -X POST '${hook.url}' \\
  -H 'Content-Type: application/json' \\
  -d '{"content": "Deploy finished ✅", "username": "CI"}'`}
                </pre>
                <p className="text-[0.7rem] mt-1.5" style={{ color: 'var(--text-faint)' }}>
                  <code>text</code> works as an alias for <code>content</code>, so Slack-style senders work
                  unchanged. Treat this URL as a secret.
                </p>
              </details>
            </div>
          )
        })}

        {!hooks.length && (
          <p className="text-sm px-4 py-10 text-center" style={{ color: 'var(--text-faint)' }}>
            No integrations yet.
          </p>
        )}
      </div>

      <Modal
        open={creating}
        onClose={() => setCreating(false)}
        title="Create a webhook"
        width="sm"
        footer={
          <>
            <button className="btn btn-ghost" onClick={() => setCreating(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={create} disabled={busy || !name.trim() || !channelId}>
              {busy ? 'Creating…' : 'Create'}
            </button>
          </>
        }
      >
        <div className="px-5 py-4 space-y-4">
          <Field label="Name" hint="Shown as the sender's name unless the request overrides it.">
            <input className="input" value={name} autoFocus onChange={(e) => setName(e.target.value)} placeholder="CI bot" />
          </Field>
          <Field label="Channel">
            <Select
              value={channelId}
              onChange={setChannelId}
              ariaLabel="Channel"
              options={channels
                .filter((channel) => channel.kind !== 'voice')
                .map((channel) => ({ value: channel.id, label: `#${channel.name}` }))}
            />
          </Field>
        </div>
      </Modal>
    </div>
  )
}
