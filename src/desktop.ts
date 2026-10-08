/**
 * Bridge to the desktop shell (electron/preload.cjs). In a guest's browser it is absent
 * and every helper degrades to a no-op.
 */

export type UpdateState =
  /** Not packaged (development) or running in a browser. */
  | { state: 'unsupported' }
  | { state: 'idle' }
  | { state: 'checking' }
  | { state: 'latest'; checkedAt: number }
  | { state: 'downloading'; version: string; percent: number }
  | { state: 'ready'; version: string }
  | { state: 'error'; message: string }

export type Theme = 'system' | 'light' | 'dark'

export interface TrattoBridge {
  desktop: true
  platform: string
  setTheme(theme: Theme): void
  /** Whole-window zoom, 1 = 100%. Used by the "interface size" accessibility setting. */
  setZoom(factor: number): void
  getInfo(): Promise<{ version: string }>
  getUpdateState(): Promise<UpdateState>
  checkForUpdates(): Promise<UpdateState>
  /** Quits, installs the downloaded update and reopens Tratto. */
  installUpdate(): void
  /** Subscribes to updater progress; returns the unsubscribe function. */
  onUpdate(cb: (s: UpdateState) => void): () => void
}

declare global {
  interface Window {
    tratto?: TrattoBridge
  }
}

export const bridge: TrattoBridge | null = typeof window !== 'undefined' ? (window.tratto ?? null) : null
export const isDesktop = !!bridge
