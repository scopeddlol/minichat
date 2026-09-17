import { useEffect, useState } from 'react'
import Auth from './routes/Auth'
import Chat from './routes/Chat'
import Setup from './routes/Setup'
import { ConfirmProvider, Spinner, ToastViewport } from './components/ui'
import { applyTheme, effectiveTheme } from './lib/theme'
import { useStore } from './lib/store'

/** Path-based routing without a router: the app only has one real route. */
function readInviteCode(): string | null {
  const match = location.pathname.match(/^\/invite\/([A-Za-z0-9]+)/)
  return match ? match[1] : null
}

export default function App() {
  const phase = useStore((s) => s.phase)
  const meta = useStore((s) => s.meta)
  const bootError = useStore((s) => s.bootError)
  const boot = useStore((s) => s.boot)
  const [inviteCode] = useState(readInviteCode)

  useEffect(() => {
    applyTheme()
    const media = window.matchMedia('(prefers-color-scheme: light)')
    const onChange = () => {
      if (effectiveTheme() === 'system') applyTheme('system')
    }
    media.addEventListener('change', onChange)
    void boot()
    return () => media.removeEventListener('change', onChange)
  }, [boot])

  return (
    <ConfirmProvider>
      {phase === 'loading' && (
        <div className="h-full flex items-center justify-center" style={{ color: 'var(--text-faint)' }}>
          <Spinner size={24} />
        </div>
      )}

      {phase === 'error' && (
        <div className="h-full flex items-center justify-center p-6">
          <div className="card p-8 text-center max-w-sm">
            <h1 className="text-lg font-semibold">Can't reach the server</h1>
            <p className="text-sm mt-2" style={{ color: 'var(--text-muted)' }}>
              {bootError}
            </p>
            <button className="btn btn-primary mt-5 w-full" onClick={() => location.reload()}>
              Try again
            </button>
          </div>
        </div>
      )}

      {phase === 'setup' && meta && <Setup meta={meta} />}
      {phase === 'anonymous' && meta && <Auth meta={meta} inviteCode={inviteCode} />}
      {phase === 'ready' && <Chat />}

      <ToastViewport />
    </ConfirmProvider>
  )
}
