import { getToken } from './api'

export type GatewayEvent = { t: string; d: any }
type Listener = (event: GatewayEvent) => void
export type GatewayStatus = 'connecting' | 'ready' | 'reconnecting' | 'closed'

/**
 * WebSocket client with exponential backoff. The server expects an `identify`
 * frame before it will send anything, which keeps the token out of the URL.
 */
class Gateway {
  private socket: WebSocket | null = null
  private listeners = new Set<Listener>()
  private statusListeners = new Set<(status: GatewayStatus) => void>()
  private attempt = 0
  private reconnectTimer: number | null = null
  private heartbeat: number | null = null
  private deliberateClose = false
  private _status: GatewayStatus = 'closed'

  get status() {
    return this._status
  }

  private setStatus(status: GatewayStatus) {
    if (this._status === status) return
    this._status = status
    this.statusListeners.forEach((listener) => listener(status))
  }

  connect() {
    const token = getToken()
    if (!token) return
    if (this.socket && (this.socket.readyState === WebSocket.OPEN || this.socket.readyState === WebSocket.CONNECTING)) {
      return
    }

    this.deliberateClose = false
    this.setStatus(this.attempt === 0 ? 'connecting' : 'reconnecting')

    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
    const socket = new WebSocket(`${protocol}//${location.host}/api/gateway`)
    this.socket = socket

    socket.onopen = () => {
      socket.send(JSON.stringify({ op: 'identify', token }))
      this.heartbeat = window.setInterval(() => {
        if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ op: 'ping' }))
      }, 25_000)
    }

    socket.onmessage = (event) => {
      let payload: GatewayEvent
      try {
        payload = JSON.parse(event.data)
      } catch {
        return
      }
      if (payload.t === 'READY') {
        this.attempt = 0
        this.setStatus('ready')
      }
      this.listeners.forEach((listener) => listener(payload))
    }

    socket.onclose = () => {
      this.clearHeartbeat()
      this.socket = null
      if (this.deliberateClose) {
        this.setStatus('closed')
        return
      }
      this.setStatus('reconnecting')
      this.scheduleReconnect()
    }

    socket.onerror = () => socket.close()
  }

  private scheduleReconnect() {
    if (this.reconnectTimer !== null) return
    // Back off to at most 20s, with jitter so a server restart doesn't bring
    // every client back in the same instant.
    const base = Math.min(1000 * 2 ** this.attempt, 20_000)
    const delay = base + Math.random() * 800
    this.attempt += 1
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null
      this.connect()
    }, delay)
  }

  private clearHeartbeat() {
    if (this.heartbeat !== null) {
      clearInterval(this.heartbeat)
      this.heartbeat = null
    }
  }

  send(payload: Record<string, unknown>) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(payload))
    }
  }

  disconnect() {
    this.deliberateClose = true
    this.attempt = 0
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    this.clearHeartbeat()
    this.socket?.close()
    this.socket = null
    this.setStatus('closed')
  }

  /** Force an immediate retry, e.g. when the tab regains focus. */
  refresh() {
    if (this._status === 'ready') return
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    this.attempt = 0
    this.connect()
  }

  on(listener: Listener) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  onStatus(listener: (status: GatewayStatus) => void) {
    this.statusListeners.add(listener)
    return () => this.statusListeners.delete(listener)
  }
}

export const gateway = new Gateway()
