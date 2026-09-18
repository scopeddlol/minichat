import { useEffect, useState } from 'react'
import { api, ApiError } from '../lib/api'
import { useStore } from '../lib/store'
import type { Category } from '../lib/types'
import { RolePicker } from './ChannelDialog'
import { Field, Modal, Switch, toast } from './ui'

/**
 * Create or edit a category.
 *
 * A private category is the useful half of this: every channel created inside
 * it starts synced, so "staff only" is set once rather than remembered every
 * time someone adds a channel.
 */
export default function CategoryDialog({
  open,
  onClose,
  category,
}: {
  open: boolean
  onClose: () => void
  /** Editing an existing category; omit to create one. */
  category?: Category | null
}) {
  const roles = useStore((s) => s.roles)
  const editing = !!category

  const [name, setName] = useState('')
  const [isPrivate, setIsPrivate] = useState(false)
  const [allowed, setAllowed] = useState<string[]>([])
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!open) return
    setName(category?.name ?? '')
    setIsPrivate(category?.is_private ?? false)
    setAllowed([])
    if (category?.is_private) {
      api
        .allowedRoles(category.id)
        .then((result) => setAllowed(result.role_ids))
        .catch(() => undefined)
    }
  }, [open, category])

  const save = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setBusy(true)
    try {
      if (category) {
        await api.updateCategory(category.id, {
          name: trimmed,
          is_private: isPrivate,
          allowed_role_ids: allowed,
        })
        toast.success('Category saved.')
      } else {
        await api.createCategory({
          name: trimmed,
          is_private: isPrivate,
          allowed_role_ids: allowed,
        })
        toast.success(`${trimmed} created.`)
      }
      onClose()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : 'Could not save the category.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={editing ? `Edit ${category?.name}` : 'Create a category'}
      description="Categories group channels in the sidebar."
      width="md"
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={save} disabled={busy || !name.trim()}>
            {busy ? 'Saving…' : editing ? 'Save category' : 'Create category'}
          </button>
        </>
      }
    >
      <div className="px-5 py-4 space-y-4">
        <Field label="Name">
          <input
            className="input"
            value={name}
            autoFocus
            maxLength={48}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && void save()}
            placeholder="Staff Area"
          />
        </Field>

        <Switch
          checked={isPrivate}
          onChange={setIsPrivate}
          label="Private category"
          hint="Hidden from everyone except the roles you pick. Channels added to it start with the same access."
        />

        {isPrivate && <RolePicker roles={roles} value={allowed} onChange={setAllowed} />}

        {editing && isPrivate && (
          <p className="text-xs leading-relaxed" style={{ color: 'var(--text-muted)' }}>
            Saving applies this to every channel in the category that is set to use the
            category's permissions. Channels with their own permissions are left alone.
          </p>
        )}
      </div>
    </Modal>
  )
}
