import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { VitePWA } from 'vite-plugin-pwa'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['favicon.svg', 'icon-192.png', 'icon-512.png', 'apple-touch-icon.png'],
      manifest: {
        name: 'MiniChat',
        short_name: 'MiniChat',
        description: 'Voice, video and text chat for a single community.',
        theme_color: '#0d0f16',
        background_color: '#0d0f16',
        display: 'standalone',
        orientation: 'portrait-primary',
        start_url: '/',
        scope: '/',
        categories: ['social', 'communication'],
        icons: [
          { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        ],
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,png,woff2}'],
        // The API and gateway must never be served from cache: stale messages
        // are worse than no messages.
        navigateFallbackDenylist: [/^\/api/, /^\/uploads/, /^\/webhooks/],
        runtimeCaching: [
          {
            urlPattern: /\/uploads\/.*/,
            handler: 'CacheFirst',
            options: {
              cacheName: 'minichat-uploads',
              expiration: { maxEntries: 300, maxAgeSeconds: 60 * 60 * 24 * 30 },
            },
          },
        ],
      },
      devOptions: { enabled: false },
    }),
  ],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true, ws: true },
      '/uploads': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/webhooks': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
    rollupOptions: {
      output: {
        manualChunks: {
          // LiveKit is large and only needed once someone joins voice.
          livekit: ['livekit-client'],
        },
      },
    },
  },
})
