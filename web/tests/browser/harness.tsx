import React, { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import Chat from '../../src/routes/Chat'
import { ContextMenuProvider } from '../../src/components/ContextMenu'
import { ConfirmProvider, ToastViewport } from '../../src/components/ui'
import { useStore } from '../../src/lib/store'
import { useVoice } from '../../src/lib/voice'
import { playVoiceSound } from '../../src/lib/voiceSounds'
import '../../src/index.css'
const member = { id: 'me', username: 'operator', display_name: 'Operator', roles: [], is_operator: true, presence: 'online', accent_color: '#5577aa', avatar_frame: {}, banner_frame: {} }
const peer = { ...member, id: 'peer', username: 'member', display_name: 'Test Member', is_operator: false }
const channel = { id: 'general', name: 'General', kind: 'text', category_id: null, position: 0, topic: '', description: '', emoji: '', slowmode: 0 }
const message = { id: 'msg', channel_id: 'general', content: 'Hello world', author: member, created_at: '2026-09-19 08:00:00', attachments: [], reactions: [] }
useStore.setState({ phase: 'ready', connection: 'ready', me: member, members: { me: member, peer }, channels: [channel, { ...channel, id: 'other', name: 'Other', position: 1 }], categories: [{ id: 'category', name: 'Games', position: 0 }], activeChannelId: 'general', permissions: 1n << 30n, channelPermissions: { general: 1n << 30n }, instance: { name: 'Test Community', tagline: '' }, messages: { general: [message], other: [] }, voiceEnabled: true } as any)
Object.assign(window, { fixtureVoice: useVoice, playVoiceSound })
createRoot(document.getElementById('root')!).render(<StrictMode><ConfirmProvider><ContextMenuProvider><Chat/><ToastViewport/></ContextMenuProvider></ConfirmProvider></StrictMode>)
