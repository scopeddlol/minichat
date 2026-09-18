export type ContextAction = { label: string; run: () => void | Promise<unknown> }
export interface ContextRequest {
  x: number
  y: number
  title: string
  items: ContextAction[]
  trigger: HTMLElement
}
export function showContextMenu(menu: ContextRequest) {
  window.dispatchEvent(new CustomEvent('minichat:context-menu', { detail: menu }))
}
