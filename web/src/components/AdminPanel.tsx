import {
  Hash, LayoutDashboard, Link2, Palette, ScrollText, Shield, Smile, Users, Webhook, X,
} from 'lucide-react'
import { useState } from 'react'
import { can, P } from '../lib/perms'
import { useStore } from '../lib/store'
import AuditLog from './admin/AuditLog'
import Branding from './admin/Branding'
import Channels from './admin/Channels'
import Emoji from './admin/Emoji'
import Integrations from './admin/Integrations'
import Invites from './admin/Invites'
import Members from './admin/Members'
import Overview from './admin/Overview'
import Roles from './admin/Roles'
import Select from './Select'
import { Modal } from './ui'

type Tab =
  | 'overview' | 'branding' | 'channels' | 'roles' | 'members' | 'emoji'
  | 'invites' | 'integrations' | 'audit'

export default function AdminPanel({
  open,
  onClose,
  onOpenProfile,
}: {
  open: boolean
  onClose: () => void
  onOpenProfile: (userId: string) => void
}) {
  const permissions = useStore((s) => s.permissions)
  const instance = useStore((s) => s.instance)
  const [tab, setTab] = useState<Tab>('overview')

  const tabs: { key: Tab; label: string; icon: React.ReactNode; visible: boolean }[] = [
    {
      key: 'overview',
      label: 'Overview',
      icon: <LayoutDashboard size={15} />,
      visible: can(permissions, P.MANAGE_INSTANCE),
    },
    { key: 'branding', label: 'Instance', icon: <Palette size={15} />, visible: can(permissions, P.MANAGE_INSTANCE) },
    { key: 'channels', label: 'Channels', icon: <Hash size={15} />, visible: can(permissions, P.MANAGE_CHANNELS) },
    { key: 'roles', label: 'Roles', icon: <Shield size={15} />, visible: can(permissions, P.MANAGE_ROLES) },
    {
      key: 'members',
      label: 'Members',
      icon: <Users size={15} />,
      visible: can(permissions, P.KICK_MEMBERS) || can(permissions, P.BAN_MEMBERS) || can(permissions, P.MANAGE_ROLES),
    },
    { key: 'emoji', label: 'Emoji', icon: <Smile size={15} />, visible: can(permissions, P.MANAGE_EMOJI) },
    { key: 'invites', label: 'Invites', icon: <Link2 size={15} />, visible: can(permissions, P.CREATE_INVITES) },
    {
      key: 'integrations',
      label: 'Integrations',
      icon: <Webhook size={15} />,
      visible: can(permissions, P.MANAGE_WEBHOOKS),
    },
    { key: 'audit', label: 'Audit log', icon: <ScrollText size={15} />, visible: can(permissions, P.VIEW_AUDIT_LOG) },
  ]

  const available = tabs.filter((entry) => entry.visible)
  const active = available.some((entry) => entry.key === tab) ? tab : available[0]?.key

  return (
    <Modal open={open} onClose={onClose} width="full" bare>
      <div className="flex flex-col lg:flex-row" style={{ height: 'min(84vh, 820px)' }}>
        {/* On a phone the section list was a horizontal scroller showing
            three of nine tabs, with the close button parked off the right-hand
            end. A picker plus a fixed close button is reachable with a thumb. */}
        <header
          className="lg:hidden flex items-center gap-2 p-3 border-b shrink-0"
          style={{ background: 'var(--surface-0)' }}
        >
          <Shield size={16} className="shrink-0" style={{ color: 'var(--accent)' }} />
          <Select
            value={active ?? ''}
            onChange={(next) => setTab(next as Tab)}
            ariaLabel="Admin section"
            options={available.map((entry) => ({ value: entry.key, label: entry.label }))}
          />
          <button className="btn btn-ghost !p-2 shrink-0" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </header>

        <nav
          className="hidden lg:w-56 shrink-0 lg:border-r lg:flex lg:flex-col lg:overflow-y-auto scroll-thin"
          style={{ background: 'var(--surface-0)' }}
        >
          <div className="flex items-center gap-2 px-4 pt-4 pb-3">
            <Shield size={16} style={{ color: 'var(--accent)' }} />
            <div className="min-w-0">
              <p className="text-sm font-semibold truncate">Admin</p>
              <p className="text-[0.68rem] truncate" style={{ color: 'var(--text-faint)' }}>
                {instance?.name}
              </p>
            </div>
          </div>

          <div className="flex flex-col gap-1 p-2 lg:px-2 flex-1">
            {available.map((entry) => (
              <button
                key={entry.key}
                onClick={() => setTab(entry.key)}
                className="flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm transition-colors shrink-0 whitespace-nowrap"
                style={{
                  background: active === entry.key ? 'var(--accent-soft)' : 'transparent',
                  color: active === entry.key ? 'var(--accent)' : 'var(--text-muted)',
                  fontWeight: active === entry.key ? 600 : 500,
                }}
              >
                {entry.icon}
                {entry.label}
              </button>
            ))}
          </div>

        </nav>

        <div className="flex-1 min-w-0 overflow-y-auto scroll-thin" style={{ background: 'var(--bg)' }}>
          <header
            className="hidden lg:flex items-center justify-between px-6 py-3.5 border-b sticky top-0 z-[5]"
            style={{ background: 'var(--bg)' }}
          >
            <h2 className="font-semibold">{available.find((entry) => entry.key === active)?.label}</h2>
            <button className="btn btn-ghost !p-1.5" onClick={onClose} aria-label="Close">
              <X size={17} />
            </button>
          </header>

          <div className="p-4 sm:p-6">
            {active === 'overview' && <Overview />}
            {active === 'branding' && <Branding />}
            {active === 'channels' && <Channels />}
            {active === 'roles' && <Roles />}
            {active === 'members' && <Members onOpenProfile={onOpenProfile} />}
            {active === 'emoji' && <Emoji />}
            {active === 'invites' && <Invites />}
            {active === 'integrations' && <Integrations />}
            {active === 'audit' && <AuditLog />}
            {!active && (
              <p className="text-sm py-12 text-center" style={{ color: 'var(--text-faint)' }}>
                You don't have access to any admin sections.
              </p>
            )}
          </div>
        </div>
      </div>
    </Modal>
  )
}
