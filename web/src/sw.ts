/// <reference lib="webworker" />
import { cleanupOutdatedCaches, precacheAndRoute } from 'workbox-precaching'

declare const self: ServiceWorkerGlobalScope

// Injected at build time by vite-plugin-pwa.
precacheAndRoute(self.__WB_MANIFEST)
cleanupOutdatedCaches()

self.addEventListener('install', () => {
  void self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim())
})

interface PushPayload {
  title: string
  body: string
  icon?: string | null
  tag: string
  channel_id: string
  message_id: string
}

self.addEventListener('push', (event) => {
  if (!event.data) return

  let payload: PushPayload
  try {
    payload = event.data.json()
  } catch {
    payload = {
      title: 'MiniChat',
      body: event.data.text(),
      tag: 'minichat',
      channel_id: '',
      message_id: '',
    }
  }

  event.waitUntil(
    (async () => {
      // If a window is already focused, the in-app unread badge is doing the
      // job and a system notification would just be noise.
      const clients = await self.clients.matchAll({
        type: 'window',
        includeUncontrolled: true,
      })
      if (clients.some((client) => client.focused)) return

      await self.registration.showNotification(payload.title, {
        body: payload.body,
        icon: payload.icon || '/icon-192.png',
        badge: '/icon-192.png',
        // Replaces the previous notification from the same channel rather than
        // stacking one per message.
        tag: payload.tag || 'minichat',
        renotify: Boolean(payload.tag),
        data: { channelId: payload.channel_id, messageId: payload.message_id },
      } as NotificationOptions)
    })(),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const data = (event.notification.data ?? {}) as { channelId?: string; messageId?: string }

  event.waitUntil(
    (async () => {
      const clients = await self.clients.matchAll({
        type: 'window',
        includeUncontrolled: true,
      })

      // Reuse an open tab where possible instead of piling up windows.
      for (const client of clients) {
        if ('focus' in client) {
          await client.focus()
          client.postMessage({
            type: 'minichat:navigate',
            channelId: data.channelId,
            messageId: data.messageId,
          })
          return
        }
      }

      const url = data.channelId ? `/?channel=${data.channelId}` : '/'
      await self.clients.openWindow(url)
    })(),
  )
})
