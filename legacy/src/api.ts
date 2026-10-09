/** Thin fetch wrapper. On the host the session cookie authenticates; guests send their session code. */
let guestCode: string | null = null
export const setGuestCode = (code: string | null) => {
  guestCode = code
}
export const isGuest = () => guestCode !== null

export class ApiError extends Error {
  readonly status: number
  constructor(message: string, status: number) {
    super(message)
    this.status = status
  }
}

export async function api<T = unknown>(path: string, init: RequestInit & { json?: unknown } = {}): Promise<T> {
  const headers = new Headers(init.headers)
  if (guestCode && !headers.has('Authorization')) headers.set('Authorization', `Bearer ${guestCode}`)
  let body = init.body
  if (init.json !== undefined) {
    headers.set('Content-Type', 'application/json')
    body = JSON.stringify(init.json)
  }
  let res: Response
  try {
    res = await fetch(path, { ...init, headers, body, credentials: 'same-origin' })
  } catch {
    throw new ApiError('Impossibile raggiungere Tratto. Controlla la connessione.', 0)
  }
  if (!res.ok) {
    let message = `Errore ${res.status}`
    try {
      message = (await res.json()).error ?? message
    } catch {
      /* not JSON */
    }
    throw new ApiError(message, res.status)
  }
  const type = res.headers.get('content-type') ?? ''
  return (type.includes('json') ? res.json() : res.blob()) as Promise<T>
}

export interface BoardInfo {
  id: string
  title: string
  created_at: number
  updated_at: number
  has_thumb: number
}

export interface ShareState {
  active: boolean
  boardId?: string
  access?: 'edit' | 'view'
  status?: 'starting' | 'live' | 'error'
  error?: string | null
  url?: string | null
  autoAdmit?: boolean
  /** People waiting to be let in. */
  requests?: { ticket: string; name: string }[]
  /** People let in; `online` while they have the board open. */
  guests?: { ticket: string; name: string; online: boolean }[]
}

export const fetchFile = (boardId: string, fileId: string) => api<Blob>(`/api/boards/${boardId}/files/${fileId}`)

const MAX_SIDE = 2560

/**
 * Downscales big pictures and re-encodes them as WebP before upload: boards stay light
 * and the server never stores anything but plain raster images.
 */
export async function uploadImage(boardId: string, file: Blob) {
  const bitmap = await createImageBitmap(file).catch(() => {
    throw new ApiError('Questa immagine non si può aprire. Usa PNG, JPG, GIF o WebP.', 415)
  })
  const k = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height))
  const w = Math.max(1, Math.round(bitmap.width * k))
  const h = Math.max(1, Math.round(bitmap.height * k))
  const canvas = new OffscreenCanvas(w, h)
  canvas.getContext('2d')!.drawImage(bitmap, 0, 0, w, h)
  let blob = await canvas.convertToBlob({ type: 'image/webp', quality: 0.9 })
  if (blob.type !== 'image/webp') blob = await canvas.convertToBlob({ type: 'image/png' })
  const scaled = k < 1 ? await createImageBitmap(canvas) : bitmap
  const { id } = await api<{ id: string }>(`/api/boards/${boardId}/files`, { method: 'POST', body: blob })
  return { id, width: w, height: h, bitmap: scaled }
}

export function timeAgo(ms: number) {
  const s = Math.round((Date.now() - ms) / 1000)
  if (s < 45) return 'adesso'
  const m = Math.round(s / 60)
  if (m < 60) return m === 1 ? '1 minuto fa' : `${m} minuti fa`
  const h = Math.round(m / 60)
  if (h < 24) return h === 1 ? '1 ora fa' : `${h} ore fa`
  const d = Math.round(h / 24)
  if (d < 7) return d === 1 ? 'ieri' : `${d} giorni fa`
  return new Date(ms).toLocaleDateString('it-IT', { day: 'numeric', month: 'short', year: d > 300 ? 'numeric' : undefined })
}
