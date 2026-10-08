import express, { type Request, type Response, type NextFunction } from 'express'
import { createServer, type IncomingMessage, type Server as HttpServer } from 'node:http'
import { randomBytes, timingSafeEqual } from 'node:crypto'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { Duplex } from 'node:stream'
import { WebSocketServer, type WebSocket } from 'ws'
import { Hocuspocus } from '@hocuspocus/server'
import * as Y from 'yjs'
import { openDb, type BoardRow } from './db.ts'
import { openTunnel, type Tunnel } from './tunnel.ts'

export interface ServerOptions {
  dataDir: string
  /** Built UI folder; null serves the UI through Vite (development). */
  staticDir: string | null
  cloudflared: string
  hostPort?: number
}

type Access = 'edit' | 'view'
/** A guest's request to join. The link alone isn't enough: the host admits each person. */
interface Ticket {
  name: string
  status: 'pending' | 'approved' | 'denied'
  /** Last time the guest checked in while waiting; abandoned requests drop out of the host's list. */
  seen: number
}
interface Share {
  boardId: string
  code: string
  access: Access
  tunnel: Tunnel | null
  status: 'starting' | 'live' | 'error'
  error: string | null
  tickets: Map<string, Ticket>
  autoAdmit: boolean
  uploadBytes: number
}
interface Ctx {
  kind: 'host' | 'guest'
  /** Admission ticket of a guest connection. */
  guestId?: string
}

const MAX_GUESTS = 20
const MAX_PENDING = 30
const MAX_GUEST_UPLOAD_BYTES = 300 * 1024 * 1024
const MAX_GUEST_MESSAGE = 4 * 1024 * 1024
const MAX_IMAGE = 15 * 1024 * 1024

const newId = (bytes = 9) => randomBytes(bytes).toString('base64url')
const ID_RE = /^[A-Za-z0-9_-]{6,64}$/

function safeEqual(a: string, b: string) {
  const ab = Buffer.from(a)
  const bb = Buffer.from(b)
  return ab.length === bb.length && timingSafeEqual(ab, bb)
}

/** Detects the image type from its first bytes; SVG and anything else is refused. */
export function sniffImage(buf: Uint8Array): string | null {
  const b = Buffer.from(buf.buffer, buf.byteOffset, buf.byteLength)
  if (b.length < 12) return null
  if (b[0] === 0x89 && b.toString('ascii', 1, 4) === 'PNG') return 'image/png'
  if (b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'image/jpeg'
  if (b.toString('ascii', 0, 4) === 'GIF8') return 'image/gif'
  if (b.toString('ascii', 0, 4) === 'RIFF' && b.toString('ascii', 8, 12) === 'WEBP') return 'image/webp'
  return null
}

function cookie(req: IncomingMessage, name: string) {
  for (const part of (req.headers.cookie ?? '').split(';')) {
    const [k, ...v] = part.trim().split('=')
    if (k === name) return v.join('=')
  }
  return ''
}

function clientIp(req: IncomingMessage) {
  // Cloudflare sets this header at its edge; through the tunnel the socket is always local.
  const cf = req.headers['cf-connecting-ip']
  return (Array.isArray(cf) ? cf[0] : cf) ?? req.socket.remoteAddress ?? '?'
}

/** Failed guest logins per IP: 20 tries per 10 minutes. */
function createLimiter() {
  const hits = new Map<string, { n: number; until: number }>()
  return {
    blocked(ip: string) {
      const h = hits.get(ip)
      return !!h && h.until > Date.now() && h.n >= 20
    },
    fail(ip: string) {
      const now = Date.now()
      if (hits.size > 10_000) hits.clear()
      const h = hits.get(ip)
      if (!h || h.until < now) hits.set(ip, { n: 1, until: now + 10 * 60_000 })
      else h.n++
    },
  }
}

const cleanTitle = (t: unknown) => (typeof t === 'string' && t.trim() ? t.trim().slice(0, 120) : 'Lavagna senza titolo')

function securityHeaders(dev: boolean, connect: string) {
  return (_req: Request, res: Response, next: NextFunction) => {
    res.set({
      'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer',
      'X-Frame-Options': 'DENY',
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Resource-Policy': 'same-origin',
      'Permissions-Policy': 'camera=(), microphone=(), geolocation=(), payment=(), usb=()',
    })
    if (!dev)
      res.set(
        'Content-Security-Policy',
        `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; font-src 'self' data:; connect-src 'self' ${connect}; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'`,
      )
    next()
  }
}

function listen(server: HttpServer, port: number): Promise<number> {
  return new Promise((ok, fail) => {
    server.once('error', fail)
    server.listen(port, '127.0.0.1', () => {
      server.off('error', fail)
      ok((server.address() as { port: number }).port)
    })
  })
}

export async function startServers(opts: ServerOptions) {
  const dev = opts.staticDir === null
  const db = openDb(join(opts.dataDir, 'tratto.db'))
  const hostToken = newId(32)
  let hostPort = 0
  let share: Share | null = null
  const limiter = createLimiter()
  /** Open guest sockets, with the guest id learned once Hocuspocus authenticates them. */
  const guests = new Map<WebSocket, { socketId: string; guestId: string | null; since: number }>()

  /** Close codes the guest UI understands: 4001 sharing ended, 4003 access removed, 4100 reconnect (permission changed). */
  const closeGuests = (match: (guestId: string | null) => boolean = () => true, code = 4001) => {
    for (const [ws, g] of guests) if (match(g.guestId)) ws.close(code, code === 4100 ? 'reconnect' : code === 4003 ? 'removed' : 'session-ended')
  }

  const stopShare = () => {
    share?.tunnel?.stop()
    share = null
    closeGuests()
  }

  const guestShare = (token: string): Share | null => {
    const s = share
    if (!s || s.status !== 'live') return null
    return safeEqual(token.split('.')[0] ?? '', s.code) ? s : null
  }

  /** Ticket id from a `code.ticket` token, only if the host admitted that guest. */
  const admitted = (s: Share, token: string) => {
    const id = token.split('.')[1] ?? ''
    return s.tickets.get(id)?.status === 'approved' ? id : null
  }

  const currentState = (boardId: string) => {
    const live = hocuspocus.documents.get(boardId)
    if (live) return Y.encodeStateAsUpdate(live)
    const row = db.prepare('SELECT ydoc FROM boards WHERE id = ?').get(boardId) as { ydoc: Uint8Array | null } | undefined
    return row?.ydoc ?? null
  }

  const hocuspocus = new Hocuspocus<Ctx>({
    quiet: true,
    debounce: 800,
    maxDebounce: 4000,
    async onAuthenticate({ context, token, documentName, connectionConfig, requestHeaders }) {
      if (context.kind === 'host') return
      const ip = requestHeaders.get('cf-connecting-ip') ?? '?'
      if (limiter.blocked(ip)) throw new Error('rate-limited')
      const s = guestShare(token)
      if (!s || s.boardId !== documentName) {
        limiter.fail(ip)
        throw new Error('unauthorized')
      }
      const ticket = admitted(s, token)
      if (!ticket) throw new Error('not admitted')
      connectionConfig.readOnly = s.access === 'view'
      context.guestId = ticket
    },
    async onLoadDocument({ document, documentName }) {
      const row = db.prepare('SELECT ydoc FROM boards WHERE id = ?').get(documentName) as { ydoc: Uint8Array | null } | undefined
      if (!row) throw new Error('board not found')
      if (row.ydoc) Y.applyUpdate(document, row.ydoc)
    },
    async onStoreDocument({ document, documentName }) {
      db.prepare('UPDATE boards SET ydoc = ?, updated_at = ? WHERE id = ?').run(Y.encodeStateAsUpdate(document), Date.now(), documentName)
    },
    async beforeHandleMessage({ context, update }) {
      if (context.kind === 'guest' && update.byteLength > MAX_GUEST_MESSAGE) throw new Error('message too large')
    },
    async connected({ context, socketId }) {
      if (context.kind !== 'guest') return
      for (const g of guests.values()) if (g.socketId === socketId) g.guestId = context.guestId ?? null
    },
  })

  /* ---------- shared handlers ---------- */

  const sendFile = (req: Request, res: Response) => {
    const row = db.prepare('SELECT mime, data FROM files WHERE board_id = ? AND id = ?').get(req.params.boardId as string, req.params.fileId as string) as
      | { mime: string; data: Uint8Array }
      | undefined
    if (!row) return res.status(404).json({ error: 'Immagine non trovata' })
    res.set({ 'Content-Type': row.mime, 'Cache-Control': 'private, max-age=31536000, immutable', 'Content-Security-Policy': "default-src 'none'" })
    res.send(Buffer.from(row.data))
  }

  /** Stores a validated image and answers with its id; returns false when the body was refused. */
  const saveFile = (boardId: string, body: unknown, res: Response): boolean => {
    if (!Buffer.isBuffer(body) || !body.length) {
      res.status(400).json({ error: 'Nessuna immagine ricevuta' })
      return false
    }
    const mime = sniffImage(body)
    if (!mime) {
      res.status(415).json({ error: 'Formato non supportato: usa PNG, JPG, GIF o WebP' })
      return false
    }
    const fileId = newId(12)
    db.prepare('INSERT INTO files (board_id, id, mime, data, created_at) VALUES (?, ?, ?, ?, ?)').run(boardId, fileId, mime, body, Date.now())
    res.json({ id: fileId })
    return true
  }

  const rawImage = express.raw({ type: () => true, limit: MAX_IMAGE })

  /* ---------- host app: only this PC, only the app window ---------- */

  const host = express()
  host.disable('x-powered-by')
  host.use((req, res, next) => {
    // Blocks DNS-rebinding: a web page can't reach us under another hostname.
    const h = req.headers.host
    if (h !== `127.0.0.1:${hostPort}` && h !== `localhost:${hostPort}`) return res.status(403).end()
    next()
  })
  host.use(securityHeaders(dev, 'ws://127.0.0.1:* ws://localhost:*'))
  host.get('/__auth', (req, res) => {
    if (!safeEqual(String(req.query.t ?? ''), hostToken)) return res.status(403).send('Link di accesso non valido. Riapri Tratto.')
    res.setHeader('Set-Cookie', `tratto_host=${hostToken}; HttpOnly; SameSite=Strict; Path=/`)
    res.redirect('/')
  })
  host.use('/api', (req, res, next) => {
    if (!safeEqual(cookie(req, 'tratto_host'), hostToken)) return res.status(401).json({ error: 'Accesso negato' })
    res.set('Cache-Control', 'no-store')
    next()
  })

  host.post('/api/boards/import', express.json({ limit: '400mb' }), (req, res) => {
    const data = req.body as { app?: string; title?: string; doc?: string; files?: { id: string; mime: string; data: string }[] }
    if (data?.app !== 'tratto' || typeof data.doc !== 'string') return res.status(400).json({ error: 'Questo file non è una lavagna di Tratto' })
    const state = Buffer.from(data.doc, 'base64')
    try {
      Y.applyUpdate(new Y.Doc(), state)
    } catch {
      return res.status(400).json({ error: 'Il file della lavagna è danneggiato' })
    }
    const id = newId()
    const now = Date.now()
    db.exec('BEGIN')
    try {
      db.prepare('INSERT INTO boards (id, title, ydoc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)').run(id, cleanTitle(data.title), state, now, now)
      for (const f of Array.isArray(data.files) ? data.files : []) {
        const buf = Buffer.from(String(f.data), 'base64')
        const mime = sniffImage(buf)
        if (mime && ID_RE.test(String(f.id))) db.prepare('INSERT OR IGNORE INTO files (board_id, id, mime, data, created_at) VALUES (?, ?, ?, ?, ?)').run(id, f.id, mime, buf, now)
      }
      db.exec('COMMIT')
    } catch (e) {
      db.exec('ROLLBACK')
      throw e
    }
    res.json({ id })
  })

  host.use('/api', express.json({ limit: '256kb' }))

  host.get('/api/boards', (_req, res) => {
    res.json(
      db.prepare('SELECT id, title, created_at, updated_at, thumbnail IS NOT NULL AS has_thumb FROM boards ORDER BY updated_at DESC').all() as unknown as BoardRow[],
    )
  })

  host.post('/api/boards', (req, res) => {
    const id = newId()
    const now = Date.now()
    db.prepare('INSERT INTO boards (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)').run(id, cleanTitle(req.body?.title), now, now)
    res.json({ id })
  })

  host.get('/api/boards/:id', (req, res) => {
    const row = db.prepare('SELECT id, title, created_at, updated_at FROM boards WHERE id = ?').get(req.params.id)
    if (!row) return res.status(404).json({ error: 'Questa lavagna non esiste più' })
    res.json(row)
  })

  host.patch('/api/boards/:id', (req, res) => {
    db.prepare('UPDATE boards SET title = ? WHERE id = ?').run(cleanTitle(req.body?.title), req.params.id)
    res.json({ ok: true })
  })

  host.delete('/api/boards/:id', (req, res) => {
    if (share?.boardId === req.params.id) stopShare()
    hocuspocus.closeConnections(req.params.id)
    db.prepare('DELETE FROM boards WHERE id = ?').run(req.params.id)
    res.json({ ok: true })
  })

  host.post('/api/boards/:id/duplicate', (req, res) => {
    const src = db.prepare('SELECT title FROM boards WHERE id = ?').get(req.params.id) as { title: string } | undefined
    if (!src) return res.status(404).json({ error: 'Questa lavagna non esiste più' })
    const id = newId()
    const now = Date.now()
    db.prepare('INSERT INTO boards (id, title, ydoc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)').run(id, cleanTitle(`${src.title} (copia)`), currentState(req.params.id), now, now)
    db.prepare('INSERT INTO files (board_id, id, mime, data, created_at) SELECT ?, id, mime, data, created_at FROM files WHERE board_id = ?').run(id, req.params.id)
    res.json({ id })
  })

  host.get('/api/boards/:id/export', (req, res) => {
    const row = db.prepare('SELECT title FROM boards WHERE id = ?').get(req.params.id) as { title: string } | undefined
    if (!row) return res.status(404).json({ error: 'Questa lavagna non esiste più' })
    const files = db.prepare('SELECT id, mime, data FROM files WHERE board_id = ?').all(req.params.id) as { id: string; mime: string; data: Uint8Array }[]
    res.json({
      app: 'tratto',
      version: 1,
      title: row.title,
      doc: Buffer.from(currentState(req.params.id) ?? new Uint8Array()).toString('base64'),
      files: files.map((f) => ({ id: f.id, mime: f.mime, data: Buffer.from(f.data).toString('base64') })),
    })
  })

  host.put('/api/boards/:id/thumbnail', express.raw({ type: () => true, limit: '3mb' }), (req, res) => {
    if (!Buffer.isBuffer(req.body) || !sniffImage(req.body)) return res.status(415).end()
    db.prepare('UPDATE boards SET thumbnail = ? WHERE id = ?').run(req.body, req.params.id)
    res.json({ ok: true })
  })

  host.get('/api/boards/:id/thumbnail', (req, res) => {
    const row = db.prepare('SELECT thumbnail FROM boards WHERE id = ?').get(req.params.id) as { thumbnail: Uint8Array | null } | undefined
    if (!row?.thumbnail) return res.status(404).end()
    res.set('Content-Type', sniffImage(row.thumbnail) ?? 'image/png').send(Buffer.from(row.thumbnail))
  })

  host.get('/api/boards/:boardId/files/:fileId', sendFile)
  host.post('/api/boards/:boardId/files', rawImage, (req, res) => {
    if (!db.prepare('SELECT 1 FROM boards WHERE id = ?').get(req.params.boardId)) return res.status(404).json({ error: 'Questa lavagna non esiste più' })
    saveFile(req.params.boardId as string, req.body, res)
  })

  host.get('/api/settings', (_req, res) => {
    const row = db.prepare("SELECT value FROM settings WHERE key = 'prefs'").get() as { value: string } | undefined
    res.json(row ? JSON.parse(row.value) : {})
  })
  host.put('/api/settings', (req, res) => {
    const value = JSON.stringify(req.body ?? {})
    if (value.length > 64_000) return res.status(413).json({ error: 'Impostazioni troppo grandi' })
    db.prepare("INSERT INTO settings (key, value) VALUES ('prefs', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value").run(value)
    res.json({ ok: true })
  })

  /* ---------- sharing ---------- */

  const shareView = () =>
    share
      ? {
          active: true,
          boardId: share.boardId,
          access: share.access,
          status: share.status,
          error: share.error,
          url: share.tunnel ? `${share.tunnel.url}/#${share.code}` : null,
          autoAdmit: share.autoAdmit,
          requests: [...share.tickets]
            .filter(([, t]) => t.status === 'pending' && Date.now() - t.seen < 15_000)
            .map(([ticket, t]) => ({ ticket, name: t.name })),
          guests: [...share.tickets]
            .filter(([, t]) => t.status === 'approved')
            .map(([ticket, t]) => ({ ticket, name: t.name, online: [...guests.values()].some((g) => g.guestId === ticket) })),
        }
      : { active: false }

  const openShareTunnel = async (s: Share) => {
    if (!existsSync(opts.cloudflared)) {
      s.status = 'error'
      s.error = 'Manca il componente di condivisione (cloudflared). Reinstalla Tratto.'
      return
    }
    try {
      const tunnel = await openTunnel(opts.cloudflared, publicPort, () => {
        if (share !== s) return
        s.status = 'error'
        s.error = 'La connessione con Cloudflare si è interrotta. Riavvia la condivisione.'
        s.tunnel = null
        closeGuests()
      })
      if (share !== s) return tunnel.stop()
      s.tunnel = tunnel
      s.status = 'live'
    } catch (e) {
      if (share !== s) return
      s.status = 'error'
      s.error = e instanceof Error ? e.message : String(e)
    }
  }

  host.get('/api/share', (_req, res) => res.json(shareView()))

  host.post('/api/share', async (req, res) => {
    const boardId = String(req.body?.boardId ?? '')
    if (!db.prepare('SELECT 1 FROM boards WHERE id = ?').get(boardId)) return res.status(404).json({ error: 'Questa lavagna non esiste più' })
    stopShare()
    const s: Share = {
      boardId,
      code: newId(16),
      access: req.body?.access === 'view' ? 'view' : 'edit',
      tunnel: null,
      status: 'starting',
      error: null,
      tickets: new Map(),
      autoAdmit: req.body?.autoAdmit === true,
      uploadBytes: 0,
    }
    share = s
    await openShareTunnel(s)
    res.json(shareView())
  })

  host.patch('/api/share', (req, res) => {
    if (!share) return res.status(409).json({ error: 'Nessuna condivisione attiva' })
    if (req.body?.access === 'edit' || req.body?.access === 'view') {
      share.access = req.body.access
      closeGuests(undefined, 4100) // guests reconnect on their own and pick up the new permission
    }
    if (typeof req.body?.autoAdmit === 'boolean') share.autoAdmit = req.body.autoAdmit
    if (req.body?.rotate) {
      // A new link: everyone, admitted or waiting, has to ask again.
      share.code = newId(16)
      share.tickets.clear()
      closeGuests(undefined, 4003)
    }
    res.json(shareView())
  })

  host.post('/api/share/admit', (req, res) => {
    const t = share?.tickets.get(String(req.body?.ticket ?? ''))
    if (t && t.status === 'pending') t.status = req.body?.allow === true ? 'approved' : 'denied'
    res.json(shareView())
  })

  host.post('/api/share/kick', (req, res) => {
    const ticket = String(req.body?.ticket ?? '')
    const t = share?.tickets.get(ticket)
    if (t) {
      t.status = 'denied'
      closeGuests((g) => g === ticket, 4003)
    }
    res.json(shareView())
  })

  host.delete('/api/share', (_req, res) => {
    stopShare()
    res.json(shareView())
  })

  host.use('/api', (_req, res) => res.status(404).json({ error: 'Risorsa non trovata' }))

  /* ---------- public app: what guests reach through the tunnel ---------- */

  const pub = express()
  pub.disable('x-powered-by')
  pub.use(securityHeaders(dev, 'wss:'))
  pub.use((_req, res, next) => {
    if (!dev) res.set('Strict-Transport-Security', 'max-age=31536000')
    next()
  })

  const guestAuth = (req: Request, res: Response, next: NextFunction) => {
    const ip = clientIp(req)
    if (limiter.blocked(ip)) return res.status(429).json({ error: 'Troppi tentativi. Riprova tra qualche minuto.' })
    const s = guestShare((req.headers.authorization ?? '').replace(/^Bearer /, ''))
    if (!s) {
      limiter.fail(ip)
      return res.status(401).json({ error: 'Questo link non è valido oppure la sessione è terminata.' })
    }
    if (req.params.boardId && req.params.boardId !== s.boardId) return res.status(403).json({ error: 'Accesso negato' })
    res.set('Cache-Control', 'no-store')
    res.locals.share = s
    next()
  }

  const requireAdmitted = (req: Request, res: Response, next: NextFunction) => {
    if (!admitted(res.locals.share as Share, (req.headers.authorization ?? '').replace(/^Bearer /, ''))) return res.status(403).json({ error: 'Non sei ancora stato fatto entrare in questa lavagna.' })
    next()
  }

  pub.get('/api/guest/session', guestAuth, (_req, res) => {
    const s = res.locals.share as Share
    const row = db.prepare('SELECT title FROM boards WHERE id = ?').get(s.boardId) as { title: string } | undefined
    res.json({ boardId: s.boardId, title: row?.title ?? 'Lavagna', access: s.access })
  })

  pub.post('/api/guest/join', guestAuth, express.json({ limit: '4kb' }), (req, res) => {
    const s = res.locals.share as Share
    const name = typeof req.body?.name === 'string' ? req.body.name.replace(/[\u0000-\u001f]/g, '').trim().slice(0, 40) : ''
    if (!name) return res.status(400).json({ error: 'Scrivi il tuo nome per entrare.' })
    const waiting = [...s.tickets.values()].filter((t) => t.status === 'pending' && Date.now() - t.seen < 15_000).length
    if (waiting >= MAX_PENDING || s.tickets.size >= 500) return res.status(429).json({ error: 'Troppe persone in attesa. Riprova tra poco.' })
    const ticket = newId(16)
    const status = s.autoAdmit ? 'approved' : 'pending'
    s.tickets.set(ticket, { name, status, seen: Date.now() })
    res.json({ ticket, status })
  })

  pub.get('/api/guest/ticket', guestAuth, (req, res) => {
    const s = res.locals.share as Share
    const t = s.tickets.get(((req.headers.authorization ?? '').split('.')[1] ?? '').trim())
    if (t) t.seen = Date.now()
    res.json({ status: t?.status ?? 'denied' })
  })

  pub.get('/api/boards/:boardId/files/:fileId', guestAuth, requireAdmitted, sendFile)
  pub.post('/api/boards/:boardId/files', guestAuth, requireAdmitted, rawImage, (req, res) => {
    const s = res.locals.share as Share
    if (s.access !== 'edit') return res.status(403).json({ error: 'Puoi solo guardare questa lavagna' })
    const size = Buffer.isBuffer(req.body) ? req.body.length : 0
    if (s.uploadBytes + size > MAX_GUEST_UPLOAD_BYTES) return res.status(429).json({ error: 'Limite di immagini raggiunto per questa sessione' })
    if (saveFile(s.boardId, req.body, res)) s.uploadBytes += size
  })
  pub.use('/api', (_req, res) => res.status(404).json({ error: 'Risorsa non trovata' }))

  /* ---------- UI files ---------- */

  if (dev) {
    const { createServer: createVite } = await import('vite')
    const vite = await createVite({ server: { middlewareMode: true, hmr: { port: 24679 } }, appType: 'spa' })
    host.use(vite.middlewares)
    pub.use(vite.middlewares)
  } else {
    const dir = opts.staticDir!
    for (const app of [host, pub]) {
      app.use('/assets', express.static(join(dir, 'assets'), { immutable: true, maxAge: '1y' }))
      app.use(express.static(dir, { maxAge: 0 }))
      app.get('/{*path}', (_req, res) => res.sendFile(join(dir, 'index.html')))
    }
  }

  /* ---------- servers and websockets ---------- */

  const hostServer = createServer(host)
  const pubServer = createServer(pub)
  const wss = new WebSocketServer({ noServer: true, maxPayload: 32 * 1024 * 1024 })

  const reject = (socket: Duplex, status: string) => {
    socket.end(`HTTP/1.1 ${status}\r\nConnection: close\r\n\r\n`)
  }

  const accept = (req: IncomingMessage, socket: Duplex, head: Buffer, ctx: Ctx) => {
    wss.handleUpgrade(req, socket, head, (ws) => {
      const headers = new Headers()
      for (const [k, v] of Object.entries(req.headers)) if (typeof v === 'string') headers.set(k, v)
      const conn = hocuspocus.handleConnection(ws, new Request(`http://localhost${req.url}`, { headers }), ctx)
      // socketId is a plain runtime field; the typings mark it private.
      if (ctx.kind === 'guest') guests.set(ws, { socketId: (conn as unknown as { socketId: string }).socketId, guestId: null, since: Date.now() })
      ws.on('message', (data: Buffer) => conn.handleMessage(new Uint8Array(data.buffer, data.byteOffset, data.byteLength)))
      ws.on('close', (code, reason) => {
        guests.delete(ws)
        conn.handleClose({ code, reason: reason.toString() })
      })
      ws.on('error', () => ws.terminate())
    })
  }

  hostServer.on('upgrade', (req, socket, head) => {
    const origin = req.headers.origin
    const okOrigin = origin === `http://127.0.0.1:${hostPort}` || origin === `http://localhost:${hostPort}`
    if (!req.url?.startsWith('/collab') || !okOrigin || !safeEqual(cookie(req, 'tratto_host'), hostToken)) return reject(socket, '403 Forbidden')
    accept(req, socket, head, { kind: 'host' })
  })

  pubServer.on('upgrade', (req, socket, head) => {
    const s = share
    const origin = req.headers.origin ?? ''
    if (!req.url?.startsWith('/collab') || !s || s.status !== 'live') return reject(socket, '403 Forbidden')
    if (!dev && origin !== s.tunnel?.url) return reject(socket, '403 Forbidden')
    if (limiter.blocked(clientIp(req))) return reject(socket, '429 Too Many Requests')
    if (guests.size >= MAX_GUESTS) return reject(socket, '503 Service Unavailable')
    accept(req, socket, head, { kind: 'guest' })
  })

  try {
    hostPort = await listen(hostServer, opts.hostPort ?? 0)
  } catch {
    hostPort = await listen(hostServer, 0) // preferred port busy
  }
  const publicPort = await listen(pubServer, 0)

  return {
    hostUrl: `http://127.0.0.1:${hostPort}/__auth?t=${hostToken}`,
    hostOrigin: `http://127.0.0.1:${hostPort}`,
    publicPort,
    async stop() {
      stopShare()
      hocuspocus.flushPendingStores()
      for (const ws of wss.clients) ws.terminate()
      await Promise.all([hostServer, pubServer].map((s) => new Promise((ok) => s.close(ok))))
      db.close()
    },
  }
}

// `npm run dev` / `npm start`: run without Electron and print the link to open in a browser.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(fileURLToPath(import.meta.url), '../..')
  const prod = process.env.NODE_ENV === 'production'
  const servers = await startServers({
    dataDir: process.env.TRATTO_DATA ?? join(root, 'data'),
    staticDir: prod ? join(root, 'dist') : null,
    cloudflared: join(root, 'bin', process.platform === 'win32' ? 'cloudflared.exe' : 'cloudflared'),
    hostPort: Number(process.env.PORT ?? 47615),
  })
  console.log(`\n  Tratto pronto: ${servers.hostUrl}\n`)
  const quit = async () => {
    await servers.stop()
    process.exit(0)
  }
  process.on('SIGINT', quit)
  process.on('SIGTERM', quit)
}
