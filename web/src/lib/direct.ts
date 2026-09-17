import { create } from 'zustand'
import { getToken, request } from './api'
import { gateway } from './gateway'

export interface Conversation { id: string; peer_id: string; last_content: string | null; unread: number }
export interface DirectMessage { id: string; conversation_id: string; author_id: string; content: string; created_at: string }
export interface DirectCall { id: string; conversation_id: string; caller_id: string; status: 'ringing' | 'accepted' | 'ended'; created_at: string }
const post = <T>(path: string, body?: unknown) => request<T>(path, { method: 'POST', body: body ? JSON.stringify(body) : undefined })
export const direct = {
  list: () => request<Conversation[]>('/direct'),
  open: (id: string) => post<Conversation>(`/direct/open/${id}`),
  history: (id: string, before?: string) => request<DirectMessage[]>(`/direct/${id}/messages${before ? `?before=${encodeURIComponent(before)}` : ''}`),
  send: (id: string, content: string) => post<DirectMessage>(`/direct/${id}/messages`, { content }),
  ack: (id: string, message_id: string) => post(`/direct/${id}/ack`, { message_id }),
  calls: () => request<DirectCall[]>('/direct/calls'),
  call: (id: string) => post<DirectCall>(`/direct/${id}/call`),
  action: (id: string, action: 'accept' | 'end') => post<DirectCall>(`/direct/calls/${id}`, { action }),
}
interface Inbox {
  open: boolean
  active: string | null
  conversations: Conversation[]
  refresh: () => Promise<void>
  openPeer: (id: string) => Promise<void>
}
export const useInbox = create<Inbox>((set) => ({
  open: false, active: null, conversations: [],
  async refresh() {
    const token = getToken()
    const conversations = await direct.list()
    if (token && token === getToken()) set({ conversations })
  },
  async openPeer(id) {
    const conversation = await direct.open(id)
    set({ active: conversation.id, open: true })
    set({ conversations: await direct.list() })
  },
}))
gateway.onStatus(status => {
  if (status === 'closed') useInbox.setState({ open: false, active: null, conversations: [] })
})
gateway.on(event => {
  if (['READY', 'DIRECT_MESSAGE', 'DIRECT_READ'].includes(event.t)) void useInbox.getState().refresh().catch(() => undefined)
  if (event.t === 'INVALID_SESSION') useInbox.setState({ open: false, active: null, conversations: [] })
})
