import { api } from './api'

/**
 * Web Push subscription management.
 *
 * On iOS this only works once the app has been added to the home screen —
 * Safari refuses push for pages running in a browser tab — so the UI leans on
 * `isSupported()` to explain that rather than failing silently.
 */

function urlBase64ToUint8Array(base64: string): Uint8Array {
  const padding = '='.repeat((4 - (base64.length % 4)) % 4)
  const normalised = (base64 + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(normalised)
  const output = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) output[i] = raw.charCodeAt(i)
  return output
}

export function isSupported(): boolean {
  return 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window
}

export function isStandalone(): boolean {
  return (
    window.matchMedia('(display-mode: standalone)').matches ||
    // iOS reports installed PWAs through a non-standard flag.
    (navigator as unknown as { standalone?: boolean }).standalone === true
  )
}

export function isIos(): boolean {
  return /iphone|ipad|ipod/i.test(navigator.userAgent)
}

export function permission(): NotificationPermission | 'unsupported' {
  if (!isSupported()) return 'unsupported'
  return Notification.permission
}

async function registration(): Promise<ServiceWorkerRegistration | null> {
  if (!('serviceWorker' in navigator)) return null
  try {
    return await navigator.serviceWorker.ready
  } catch {
    return null
  }
}

export async function currentSubscription(): Promise<PushSubscription | null> {
  const reg = await registration()
  if (!reg) return null
  try {
    return await reg.pushManager.getSubscription()
  } catch {
    return null
  }
}

export async function isEnabled(): Promise<boolean> {
  return (await currentSubscription()) !== null
}

/** Ask for permission and register a subscription with the server. */
export async function enable(): Promise<{ ok: boolean; reason?: string }> {
  if (!isSupported()) {
    return { ok: false, reason: 'This browser does not support notifications.' }
  }
  if (isIos() && !isStandalone()) {
    return {
      ok: false,
      reason: 'On iPhone and iPad, add MiniChat to your Home Screen first — Safari only allows notifications for installed apps.',
    }
  }

  const { enabled, public_key } = await api.pushKey()
  if (!enabled || !public_key) {
    return { ok: false, reason: 'This instance has not configured push notifications.' }
  }

  const result = await Notification.requestPermission()
  if (result !== 'granted') {
    return {
      ok: false,
      reason:
        result === 'denied'
          ? 'Notifications are blocked. Re-enable them in your browser settings for this site.'
          : 'Notification permission was dismissed.',
    }
  }

  const reg = await registration()
  if (!reg) return { ok: false, reason: 'The service worker is not ready yet. Try again in a moment.' }

  let subscription = await reg.pushManager.getSubscription()
  if (subscription) {
    // A key rotation invalidates old subscriptions, so re-subscribe cleanly.
    const existingKey = subscription.options.applicationServerKey
    const wanted = urlBase64ToUint8Array(public_key)
    if (!existingKey || !sameKey(existingKey, wanted)) {
      await subscription.unsubscribe().catch(() => undefined)
      subscription = null
    }
  }

  if (!subscription) {
    try {
      subscription = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(public_key) as BufferSource,
      })
    } catch (error) {
      return {
        ok: false,
        reason: error instanceof Error ? error.message : 'Could not subscribe to notifications.',
      }
    }
  }

  const payload = subscription.toJSON()
  await api.pushSubscribe({
    endpoint: subscription.endpoint,
    p256dh: payload.keys?.p256dh ?? '',
    auth: payload.keys?.auth ?? '',
    user_agent: navigator.userAgent,
  })
  return { ok: true }
}

export async function disable(): Promise<void> {
  const subscription = await currentSubscription()
  if (!subscription) return
  const endpoint = subscription.endpoint
  await subscription.unsubscribe().catch(() => undefined)
  await api.pushUnsubscribe(endpoint).catch(() => undefined)
}

function sameKey(a: ArrayBuffer, b: Uint8Array): boolean {
  const view = new Uint8Array(a)
  if (view.length !== b.length) return false
  return view.every((byte, index) => byte === b[index])
}
