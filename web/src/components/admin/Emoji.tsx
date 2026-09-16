import { Loader2, Plus, Smile, Trash2, Upload } from 'lucide-react'
import { useRef, useState } from 'react'
import { api, ApiError } from '../../lib/api'
import { formatRelative } from '../../lib/format'
import { useStore } from '../../lib/store'
import { EmptyState, Field, Modal, toast, useConfirm } from '../ui'

export default function Emoji() {
  const emojis = useStore((s) => s.emojis)
  const members = useStore((s) => s.members)
  const confirm = useConfirm()

  const [adding, setAdding] = useState(false)
  const [name, setName] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)

  const create = async () => {
    if (!file || !name.trim()) return
    setBusy(true)
    try {
      const attachment = await api.upload(file)
      await api.createEmoji(name.trim(), attachment.url)
      toast.success(`:${name.trim().toLowerCase()}: added.`)
      setAdding(false)
      setName('')
      setFile(null)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not add that emoji.')
    } finally {
      setBusy(false)
    }
  }

  const pick = (chosen: File | null) => {
    setFile(chosen)
    // Seed the name from the filename — usually what people want anyway.
    if (chosen && !name) {
      setName(chosen.name.replace(/\.[^.]+$/, '').slice(0, 32))
    }
  }

  return (
    <div className="space-y-4 animate-fade-in max-w-3xl">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="font-semibold">Custom emoji</h3>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            Use them as <code>:name:</code> in messages, or as reactions. Square images under 256px
            look best.
          </p>
        </div>
        <button className="btn btn-primary shrink-0" onClick={() => setAdding(true)}>
          <Plus size={15} /> Add emoji
        </button>
      </div>

      {emojis.length ? (
        <div className="card divide-y overflow-hidden">
          {emojis.map((emoji) => (
            <div key={emoji.id} className="flex items-center gap-3 px-4 py-2.5">
              <img
                src={emoji.url}
                alt={emoji.name}
                className="w-8 h-8 object-contain shrink-0"
                style={{ background: 'var(--surface-2)', borderRadius: 6 }}
              />
              <div className="min-w-0 flex-1">
                <input
                  className="bg-transparent outline-none text-sm font-mono w-full"
                  defaultValue={`:${emoji.name}:`}
                  onBlur={async (event) => {
                    const next = event.target.value.trim().replace(/^:|:$/g, '')
                    if (!next || next === emoji.name) {
                      event.target.value = `:${emoji.name}:`
                      return
                    }
                    try {
                      await api.renameEmoji(emoji.id, next)
                      toast.success('Emoji renamed.')
                    } catch (error) {
                      event.target.value = `:${emoji.name}:`
                      toast.error(error instanceof ApiError ? error.message : 'Rename failed.')
                    }
                  }}
                />
                <p className="text-[0.7rem]" style={{ color: 'var(--text-faint)' }}>
                  {emoji.created_by && members[emoji.created_by]
                    ? `added by ${members[emoji.created_by].display_name} · `
                    : ''}
                  {formatRelative(emoji.created_at)}
                </p>
              </div>
              <button
                className="btn btn-ghost !p-1.5 shrink-0"
                style={{ color: 'var(--danger)' }}
                title="Delete emoji"
                onClick={async () => {
                  const ok = await confirm({
                    title: `Delete :${emoji.name}:?`,
                    body: 'Messages using it will show the name as plain text, and existing reactions will lose their image.',
                    confirmLabel: 'Delete',
                    danger: true,
                  })
                  if (!ok) return
                  try {
                    await api.deleteEmoji(emoji.id)
                    toast.success('Emoji deleted.')
                  } catch {
                    toast.error('Could not delete that emoji.')
                  }
                }}
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <EmptyState
          icon={<Smile size={22} />}
          title="No custom emoji yet"
          body="Add a few — they're a surprisingly big part of how a community sounds."
        />
      )}

      <Modal
        open={adding}
        onClose={() => setAdding(false)}
        title="Add a custom emoji"
        width="sm"
        footer={
          <>
            <button className="btn btn-ghost" onClick={() => setAdding(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={create} disabled={busy || !file || !name.trim()}>
              {busy ? <Loader2 size={14} className="animate-spin" /> : null}
              {busy ? 'Adding…' : 'Add emoji'}
            </button>
          </>
        }
      >
        <div className="px-5 py-4 space-y-4">
          <Field label="Image" hint="PNG, GIF or WebP. Animated GIFs work.">
            <label
              className="flex items-center justify-center gap-3 border border-dashed rounded-xl cursor-pointer transition-colors hover:border-[var(--accent)]"
              style={{ height: 96, background: 'var(--surface-0)' }}
            >
              {file ? (
                <>
                  <img src={URL.createObjectURL(file)} alt="" className="w-12 h-12 object-contain" />
                  <span className="text-xs truncate max-w-40">{file.name}</span>
                </>
              ) : (
                <span className="flex flex-col items-center gap-1 text-xs" style={{ color: 'var(--text-faint)' }}>
                  <Upload size={17} /> Choose an image
                </span>
              )}
              <input
                ref={fileInput}
                type="file"
                accept="image/png,image/gif,image/webp,image/jpeg"
                className="hidden"
                onChange={(event) => pick(event.target.files?.[0] ?? null)}
              />
            </label>
          </Field>

          <Field label="Name" hint="Letters, numbers and underscores. Used as :name: in messages.">
            <div className="flex items-center gap-1.5">
              <span style={{ color: 'var(--text-faint)' }}>:</span>
              <input
                className="input font-mono"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="party_parrot"
              />
              <span style={{ color: 'var(--text-faint)' }}>:</span>
            </div>
          </Field>
        </div>
      </Modal>
    </div>
  )
}
