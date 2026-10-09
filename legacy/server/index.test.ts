import { describe, it, before, after } from 'node:test'
import assert from 'node:assert/strict'
import { request } from 'node:http'
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { WebSocket } from 'ws'
import * as Y from 'yjs'
import { HocuspocusProvider, HocuspocusProviderWebsocket } from '@hocuspocus/provider'
import { startServers } from './index.ts'

const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64')
const TUNNEL = 'https://test-abc.trycloudflare.com'

interface Res {
  status: number
  headers: Record<string, string | string[] | undefined>
  body: Buffer
  json: any
}

function http(port: number, method: string, path: string, headers: Record<string, string> = {}, body?: Buffer | object): Promise<Res> {
  const payload = body === undefined ? undefined : Buffer.isBuffer(body) ? body : Buffer.from(JSON.stringify(body))
  if (body !== undefined && !Buffer.isBuffer(body)) headers = { 'content-type': 'application/json', ...headers }
  return new Promise((ok, fail) => {
    const req = request({ host: '127.0.0.1', port, method, path, headers: { ...(payload ? { 'content-length': String(payload.length) } : {}), ...headers } }, (res) => {
      const chunks: Buffer[] = []
      res.on('data', (c) => chunks.push(c))
      res.on('end', () => {
        const buf = Buffer.concat(chunks)
        let json: any
        try {
          json = JSON.parse(buf.toString())
        } catch {}
        ok({ status: res.statusCode!, headers: res.headers, body: buf, json })
      })
    })
    req.on('error', fail)
    req.end(payload)
  })
}

/** Resolves with the HTTP status of a refused upgrade, or 'open' if the socket was accepted. */
function wsProbe(url: string, headers: Record<string, string>): Promise<number | 'open'> {
  return new Promise((ok) => {
    const ws = new WebSocket(url, { headers })
    ws.on('open', () => (ws.terminate(), ok('open')))
    ws.on('unexpected-response', (_req, res) => (res.resume(), ok(res.statusCode!)))
    ws.on('error', () => {})
  })
}

const until = async (cond: () => boolean | Promise<boolean>, ms = 4000) => {
  const end = Date.now() + ms
  while (Date.now() < end) {
    if (await cond()) return true
    await new Promise((r) => setTimeout(r, 25))
  }
  return false
}

describe('Tratto server', () => {
  let dir: string
  let servers: Awaited<ReturnType<typeof startServers>>
  let hp: number
  let pp: number
  let cookie: string
  const providers: { destroy(): void }[] = []

  const hostReq = (method: string, path: string, body?: Buffer | object, headers: Record<string, string> = {}) =>
    http(hp, method, path, { host: `127.0.0.1:${hp}`, cookie, ...headers }, body)
  const pubReq = (method: string, path: string, code: string | null, body?: Buffer | object, headers: Record<string, string> = {}) =>
    http(pp, method, path, { ...(code === null ? {} : { authorization: `Bearer ${code}` }), ...headers }, body)

  const newBoard = async (title = 'T') => (await hostReq('POST', '/api/boards', { title })).json.id as string
  const upload = async (boardId: string) => (await hostReq('POST', `/api/boards/${boardId}/files`, PNG)).json.id as string
  const share = async (boardId: string, access: 'edit' | 'view', extra: object = {}) => {
    const r = await hostReq('POST', '/api/share', { boardId, access, ...extra })
    return { ...r, code: String(r.json.url).split('#')[1] }
  }
  const exportMap = async (boardId: string) => {
    const e = (await hostReq('GET', `/api/boards/${boardId}/export`)).json
    const d = new Y.Doc()
    Y.applyUpdate(d, Buffer.from(e.doc, 'base64'))
    return d.getMap('m').toJSON()
  }

  let ipN = 0
  const freshIp = () => ({ 'cf-connecting-ip': `192.0.2.${++ipN}` })
  const joinReq = (code: string, name: unknown, ip = freshIp()) => pubReq('POST', '/api/guest/join', code, { name }, ip)
  const ticketOf = (code: string, ticket: string, ip = freshIp()) => pubReq('GET', '/api/guest/ticket', `${code}.${ticket}`, undefined, ip)
  const admit = (ticket: string, allow = true) => hostReq('POST', '/api/share/admit', { ticket, allow })
  /** Joins and gets admitted; returns the `code.ticket` bearer/token. */
  const admitted = async (code: string, name = 'Ospite') => {
    const j = await joinReq(code, name)
    await admit(j.json.ticket)
    return { token: `${code}.${j.json.ticket}`, ticket: j.json.ticket as string }
  }

  /** Guest Yjs client through the real provider; Origin is set on the upgrade request. */
  const guest = (boardId: string, token: string, origin = TUNNEL) => {
    class WS extends WebSocket {
      constructor(url: string, protocols?: string | string[]) {
        super(url, protocols, { origin, headers: { 'cf-connecting-ip': '203.0.113.7' } })
      }
    }
    const sock = new HocuspocusProviderWebsocket({ url: `ws://127.0.0.1:${pp}/collab`, WebSocketPolyfill: WS, maxAttempts: 1 })
    providers.push(sock)
    let onAuth!: (scope: string) => void
    let onFail!: (reason: string) => void
    const authenticated = new Promise<string>((r) => (onAuth = r))
    const failed = new Promise<string>((r) => (onFail = r))
    const provider = new HocuspocusProvider({
      websocketProvider: sock,
      name: boardId,
      token,
      onAuthenticated: ({ scope }) => onAuth(scope),
      onAuthenticationFailed: ({ reason }) => onFail(reason),
    })
    providers.push(provider)
    provider.attach()
    const timeout = <T>(p: Promise<T>) => Promise.race([p, new Promise<'timeout'>((r) => setTimeout(() => r('timeout'), 3000))])
    return { provider, authenticated: () => timeout(authenticated), failed: () => timeout(failed) }
  }

  before(async () => {
    dir = mkdtempSync(join(tmpdir(), 'tratto-test-'))
    writeFileSync(join(dir, 'index.html'), '<!doctype html><title>stub</title>')
    const fake = join(dir, 'fake-cloudflared.js')
    writeFileSync(
      fake,
      `#!/usr/bin/env node\nconsole.error('INF |  ${TUNNEL}  |');\nconsole.error('INF Registered tunnel connection connIndex=0');\nsetInterval(() => {}, 1000)\n`,
    )
    chmodSync(fake, 0o755)
    servers = await startServers({ dataDir: join(dir, 'data'), staticDir: dir, cloudflared: fake, hostPort: 0 })
    hp = Number(new URL(servers.hostUrl).port)
    pp = servers.publicPort
    const r = await http(hp, 'GET', new URL(servers.hostUrl).pathname + new URL(servers.hostUrl).search, { host: `127.0.0.1:${hp}` })
    assert.equal(r.status, 302)
    cookie = String((r.headers['set-cookie'] as string[])[0]).split(';')[0]
  })

  after(async () => {
    for (const p of providers) p.destroy()
    await servers.stop()
    rmSync(dir, { recursive: true, force: true })
  })

  describe('host access control', () => {
    it('401 without cookie', async () => {
      assert.equal((await http(hp, 'GET', '/api/boards', { host: `127.0.0.1:${hp}` })).status, 401)
      assert.equal((await http(hp, 'GET', '/api/boards', { host: `127.0.0.1:${hp}`, cookie: 'tratto_host=nope' })).status, 401)
    })
    it('403 with a foreign Host header, even with the cookie', async () => {
      assert.equal((await http(hp, 'GET', '/api/boards', { host: 'evil.example.com', cookie })).status, 403)
      assert.equal((await http(hp, 'GET', '/api/boards', { host: `127.0.0.1:${hp + 1}`, cookie })).status, 403)
    })
    it('accepts localhost:<port> as Host', async () => {
      assert.equal((await http(hp, 'GET', '/api/boards', { host: `localhost:${hp}`, cookie })).status, 200)
    })
    it('/__auth with a wrong token is 403 and sets no cookie', async () => {
      const r = await http(hp, 'GET', '/__auth?t=wrong', { host: `127.0.0.1:${hp}` })
      assert.equal(r.status, 403)
      assert.equal(r.headers['set-cookie'], undefined)
    })
    it('the auth cookie is HttpOnly + SameSite=Strict', async () => {
      const u = new URL(servers.hostUrl)
      const r = await http(hp, 'GET', u.pathname + u.search, { host: `127.0.0.1:${hp}` })
      const c = String((r.headers['set-cookie'] as string[])[0])
      assert.match(c, /HttpOnly/)
      assert.match(c, /SameSite=Strict/)
    })
    it('host /collab refused without cookie or with a foreign Origin', async () => {
      const url = `ws://127.0.0.1:${hp}/collab`
      assert.equal(await wsProbe(url, { origin: servers.hostOrigin }), 403)
      assert.equal(await wsProbe(url, { origin: 'https://evil.example.com', cookie }), 403)
      assert.equal(await wsProbe(url, { origin: servers.hostOrigin, cookie }), 'open')
    })
  })

  describe('boards', () => {
    it('create / list / rename / delete', async () => {
      const id = await newBoard('  Prima  ')
      let list = (await hostReq('GET', '/api/boards')).json
      assert.equal(list.find((b: any) => b.id === id).title, 'Prima')
      assert.equal((await hostReq('PATCH', `/api/boards/${id}`, { title: 'Seconda' })).status, 200)
      assert.equal((await hostReq('GET', `/api/boards/${id}`)).json.title, 'Seconda')
      assert.equal((await hostReq('DELETE', `/api/boards/${id}`)).status, 200)
      list = (await hostReq('GET', '/api/boards')).json
      assert.ok(!list.some((b: any) => b.id === id))
      assert.equal((await hostReq('GET', `/api/boards/${id}`)).status, 404)
    })
    it('empty / non-string title falls back to the default; long title is capped', async () => {
      const a = await newBoard('   ')
      const b = (await hostReq('POST', '/api/boards', { title: 'x'.repeat(500) })).json.id
      assert.equal((await hostReq('GET', `/api/boards/${a}`)).json.title, 'Lavagna senza titolo')
      assert.equal((await hostReq('GET', `/api/boards/${b}`)).json.title.length, 120)
    })
    it('settings round trip', async () => {
      assert.equal((await hostReq('PUT', '/api/settings', { theme: 'dark' })).status, 200)
      assert.deepEqual((await hostReq('GET', '/api/settings')).json, { theme: 'dark' })
    })
  })

  describe('images', () => {
    it('rejects SVG, HTML, garbage, and empty bodies; accepts a real PNG', async () => {
      const id = await newBoard()
      const svg = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"></svg>')
      const html = Buffer.from('<html><script>alert(1)</script></html>')
      assert.equal((await hostReq('POST', `/api/boards/${id}/files`, svg, { 'content-type': 'image/png' })).status, 415)
      assert.equal((await hostReq('POST', `/api/boards/${id}/files`, html, { 'content-type': 'image/png' })).status, 415)
      assert.equal((await hostReq('POST', `/api/boards/${id}/files`, Buffer.alloc(64, 7))).status, 415)
      assert.equal((await hostReq('POST', `/api/boards/${id}/files`, Buffer.alloc(0))).status, 400)
      assert.equal((await hostReq('POST', `/api/boards/${id}/files`, PNG)).status, 200)
    })
    it('upload to a missing board is 404', async () => {
      assert.equal((await hostReq('POST', '/api/boards/doesnotexist/files', PNG)).status, 404)
    })
    it('served files carry nosniff, a CSP and the sniffed mime', async () => {
      const id = await newBoard()
      const f = await upload(id)
      const r = await hostReq('GET', `/api/boards/${id}/files/${f}`)
      assert.equal(r.status, 200)
      assert.equal(r.headers['content-type'], 'image/png')
      assert.equal(r.headers['x-content-type-options'], 'nosniff')
      assert.match(String(r.headers['content-security-policy']), /default-src 'none'/)
      assert.deepEqual(r.body, PNG)
    })
    it('UI pages are served with CSP and nosniff in production mode', async () => {
      const r = await http(hp, 'GET', '/', { host: `127.0.0.1:${hp}` })
      assert.equal(r.status, 200)
      assert.match(String(r.headers['content-security-policy']), /script-src 'self'/)
      assert.equal(r.headers['x-content-type-options'], 'nosniff')
      const p = await http(pp, 'GET', '/', {})
      assert.match(String(p.headers['content-security-policy']), /script-src 'self'/)
      assert.ok(p.headers['strict-transport-security'])
    })
    it('thumbnail only accepts images', async () => {
      const id = await newBoard()
      assert.equal((await hostReq('PUT', `/api/boards/${id}/thumbnail`, Buffer.from('<svg/>................'))).status, 415)
      assert.equal((await hostReq('PUT', `/api/boards/${id}/thumbnail`, PNG)).status, 200)
      assert.equal((await hostReq('GET', `/api/boards/${id}/thumbnail`)).status, 200)
    })
  })

  describe('duplicate / export / import', () => {
    it('duplicate copies the image files', async () => {
      const id = await newBoard('Orig')
      const f = await upload(id)
      const dup = (await hostReq('POST', `/api/boards/${id}/duplicate`)).json.id
      assert.notEqual(dup, id)
      assert.equal((await hostReq('GET', `/api/boards/${dup}`)).json.title, 'Orig (copia)')
      const r = await hostReq('GET', `/api/boards/${dup}/files/${f}`)
      assert.equal(r.status, 200)
      assert.deepEqual(r.body, PNG)
      assert.equal((await hostReq('POST', '/api/boards/missing/duplicate')).status, 404)
    })
    it('export -> import keeps Yjs content and files', async () => {
      const id = await newBoard('Round')
      const f = await upload(id)
      const d = new Y.Doc()
      d.getMap('m').set('hello', 'wörld ✓')
      const doc = Buffer.from(Y.encodeStateAsUpdate(d)).toString('base64')
      const imp0 = await hostReq('POST', '/api/boards/import', { app: 'tratto', version: 1, title: 'Seed', doc, files: [{ id: f, mime: 'image/png', data: PNG.toString('base64') }] })
      assert.equal(imp0.status, 200)
      const exp = (await hostReq('GET', `/api/boards/${imp0.json.id}/export`)).json
      assert.equal(exp.app, 'tratto')
      assert.equal(exp.files.length, 1)
      const imp = await hostReq('POST', '/api/boards/import', exp)
      assert.equal(imp.status, 200)
      assert.deepEqual(await exportMap(imp.json.id), { hello: 'wörld ✓' })
      assert.deepEqual((await hostReq('GET', `/api/boards/${imp.json.id}/files/${f}`)).body, PNG)
    })
    it('import rejects foreign / corrupt files and drops non-image attachments', async () => {
      assert.equal((await hostReq('POST', '/api/boards/import', { app: 'other', doc: '' })).status, 400)
      assert.equal((await hostReq('POST', '/api/boards/import', { app: 'tratto', doc: Buffer.from('not a yjs update at all').toString('base64') })).status, 400)
      const doc = Buffer.from(Y.encodeStateAsUpdate(new Y.Doc())).toString('base64')
      const r = await hostReq('POST', '/api/boards/import', { app: 'tratto', doc, files: [{ id: 'evilsvg1', mime: 'image/svg+xml', data: Buffer.from('<svg onload=1 />......').toString('base64') }] })
      assert.equal(r.status, 200)
      assert.equal((await hostReq('GET', `/api/boards/${r.json.id}/files/evilsvg1`)).status, 404)
    })
  })

  describe('sharing: public HTTP', () => {
    it('start returns https tunnel URL with #code; session ok / 401 / 429', async () => {
      const id = await newBoard('Shared')
      const s = await share(id, 'edit')
      assert.equal(s.json.status, 'live')
      assert.equal(s.json.autoAdmit, false)
      assert.match(s.json.url, /^https:\/\/test-abc\.trycloudflare\.com\/#[A-Za-z0-9_-]{16,}$/)
      const ok = await pubReq('GET', '/api/guest/session', s.code, undefined, { 'cf-connecting-ip': '198.51.100.1' })
      assert.equal(ok.status, 200)
      assert.deepEqual(ok.json, { boardId: id, title: 'Shared', access: 'edit' })
      assert.equal((await pubReq('GET', '/api/guest/session', 'wrong')).status, 401)
      assert.equal((await pubReq('GET', '/api/guest/session', null)).status, 401)
      for (let i = 0; i < 20; i++) assert.equal((await pubReq('GET', '/api/guest/session', 'bad', undefined, { 'cf-connecting-ip': '198.51.100.9' })).status, 401)
      assert.equal((await pubReq('GET', '/api/guest/session', 'bad', undefined, { 'cf-connecting-ip': '198.51.100.9' })).status, 429)
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, { 'cf-connecting-ip': '198.51.100.9' })).status, 429)
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, { 'cf-connecting-ip': '198.51.100.10' })).status, 200)
    })

    it('join: name required, trimmed to 40, control chars stripped; wrong code 401', async () => {
      const s = await share(await newBoard(), 'edit')
      assert.equal((await joinReq(s.code, '')).status, 400)
      assert.equal((await joinReq(s.code, '   ')).status, 400)
      assert.equal((await joinReq(s.code, 42)).status, 400)
      assert.equal((await joinReq('wrong', 'Ann')).status, 401)
      const j = await joinReq(s.code, `\u0007\u0000  ${'é'.repeat(60)}  `)
      assert.equal(j.status, 200)
      assert.equal(j.json.status, 'pending')
      const req = (await hostReq('GET', '/api/share')).json.requests.find((r: any) => r.ticket === j.json.ticket)
      assert.equal(req.name, 'é'.repeat(40))
      assert.ok(!/[\u0000-\u001f]/.test(req.name))
    })

    it('pending ticket: cannot read or upload files (403); admit -> works; deny -> denied', async () => {
      const id = await newBoard()
      const f = await upload(id)
      const s = await share(id, 'edit')
      const j = (await joinReq(s.code, 'Ann')).json
      const tok = `${s.code}.${j.ticket}`
      assert.equal(j.status, 'pending')
      assert.equal((await ticketOf(s.code, j.ticket)).json.status, 'pending')
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, tok, undefined, freshIp())).status, 403)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, tok, PNG, freshIp())).status, 403)
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, s.code, undefined, freshIp())).status, 403) // code alone is not enough
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, `${s.code}.bogus`, undefined, freshIp())).status, 403)
      const view = (await hostReq('GET', '/api/share')).json
      assert.ok(view.requests.some((r: any) => r.ticket === j.ticket && r.name === 'Ann'))
      assert.equal(view.guests.length, 0)
      await admit(j.ticket, true)
      assert.equal((await ticketOf(s.code, j.ticket)).json.status, 'approved')
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, tok, undefined, freshIp())).status, 200)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, tok, PNG, freshIp())).status, 200)
      const after = (await hostReq('GET', '/api/share')).json
      assert.deepEqual(after.requests, [])
      assert.deepEqual(after.guests, [{ ticket: j.ticket, name: 'Ann', online: false }])
      // deny
      const d = (await joinReq(s.code, 'Bob')).json
      await admit(d.ticket, false)
      assert.equal((await ticketOf(s.code, d.ticket)).json.status, 'denied')
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, `${s.code}.${d.ticket}`, undefined, freshIp())).status, 403)
      assert.equal((await ticketOf(s.code, 'unknownticket')).json.status, 'denied')
      // an approved ticket can't be flipped through admit
      await admit(j.ticket, false)
      assert.equal((await ticketOf(s.code, j.ticket)).json.status, 'approved')
    })

    it('autoAdmit at start and via PATCH', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit', { autoAdmit: true })
      assert.equal(s.json.autoAdmit, true)
      assert.equal((await joinReq(s.code, 'Auto')).json.status, 'approved')
      assert.equal((await hostReq('PATCH', '/api/share', { autoAdmit: false })).json.autoAdmit, false)
      assert.equal((await joinReq(s.code, 'Manual')).json.status, 'pending')
      assert.equal((await hostReq('PATCH', '/api/share', { autoAdmit: true })).json.autoAdmit, true)
      assert.equal((await joinReq(s.code, 'Auto2')).json.status, 'approved')
    })

    it('at most 30 pending requests (429)', async () => {
      const s = await share(await newBoard(), 'edit')
      for (let i = 0; i < 30; i++) assert.equal((await joinReq(s.code, `g${i}`)).status, 200)
      assert.equal((await joinReq(s.code, 'one too many')).status, 429)
    })

    it('kick: revoked, rejoin with same ticket impossible, new join is a new pending request', async () => {
      const id = await newBoard()
      const f = await upload(id)
      const s = await share(id, 'edit')
      const a = await admitted(s.code, 'Kim')
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, a.token, undefined, freshIp())).status, 200)
      const v = (await hostReq('POST', '/api/share/kick', { ticket: a.ticket })).json
      assert.ok(!v.guests.some((g: any) => g.ticket === a.ticket))
      assert.equal((await ticketOf(s.code, a.ticket)).json.status, 'denied')
      assert.equal((await pubReq('GET', `/api/boards/${id}/files/${f}`, a.token, undefined, freshIp())).status, 403)
      const again = (await joinReq(s.code, 'Kim')).json
      assert.notEqual(again.ticket, a.ticket)
      assert.equal(again.status, 'pending')
    })

    it('rotate clears every ticket (admitted and pending)', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      const a = await admitted(s.code)
      const p = (await joinReq(s.code, 'Waiting')).json
      const rot = (await hostReq('PATCH', '/api/share', { rotate: true })).json
      const fresh = String(rot.url).split('#')[1]
      assert.deepEqual(rot.requests, [])
      assert.deepEqual(rot.guests, [])
      assert.equal((await ticketOf(fresh, a.ticket)).json.status, 'denied')
      assert.equal((await ticketOf(fresh, p.ticket)).json.status, 'denied')
      assert.equal((await ticketOf(s.code, a.ticket)).status, 401)
    })

    it('guest upload: edit accepts PNG / rejects SVG (415); view-only is 403', async () => {
      const id = await newBoard('U')
      const e = await share(id, 'edit', { autoAdmit: true })
      const a = await admitted(e.code)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, a.token, PNG, freshIp())).status, 200)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, a.token, Buffer.from('<svg onload=1></svg>'), freshIp())).status, 415)
      const v = await share(id, 'view')
      const b = await admitted(v.code)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, b.token, PNG, freshIp())).status, 403)
    })

    it('guest cannot read files of another board (403)', async () => {
      const a = await newBoard('A')
      const b = await newBoard('B')
      const fa = await upload(a)
      const fb = await upload(b)
      const s = await share(a, 'edit')
      const g = await admitted(s.code)
      assert.equal((await pubReq('GET', `/api/boards/${a}/files/${fa}`, g.token, undefined, freshIp())).status, 200)
      assert.equal((await pubReq('GET', `/api/boards/${b}/files/${fb}`, g.token, undefined, freshIp())).status, 403)
      assert.equal((await pubReq('GET', `/api/boards/${a}/files/${fb}`, g.token, undefined, freshIp())).status, 404)
    })

    it('refused (415) uploads do not count toward the 300 MB cap', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      const g = await admitted(s.code)
      const junk = Buffer.alloc(14 * 1024 * 1024, 7) // 21 x 14 MB = 294 MB... plus the PNG would still fit; use 22 to exceed 300 MB if they were counted
      for (let i = 0; i < 22; i++) assert.equal((await pubReq('POST', `/api/boards/${id}/files`, g.token, junk, freshIp())).status, 415, `junk upload ${i}`)
      assert.equal((await pubReq('POST', `/api/boards/${id}/files`, g.token, PNG, freshIp())).status, 200)
    })

    it('guest cannot reach the host API', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      assert.equal((await pubReq('GET', '/api/boards', s.code)).status, 404)
      assert.equal((await pubReq('POST', '/api/share', s.code, { boardId: id })).status, 404)
      assert.equal((await pubReq('POST', '/api/share/admit', s.code, { ticket: 'x', allow: true })).status, 404)
    })

    it('rotate invalidates the old code; delete invalidates everything', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      const ip = { 'cf-connecting-ip': '198.51.100.40' }
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, ip)).status, 200)
      const rot = await hostReq('PATCH', '/api/share', { rotate: true })
      const fresh = String(rot.json.url).split('#')[1]
      assert.notEqual(fresh, s.code)
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, ip)).status, 401)
      assert.equal((await pubReq('GET', '/api/guest/session', fresh, undefined, ip)).status, 200)
      assert.equal((await hostReq('DELETE', '/api/share')).json.active, false)
      assert.equal((await pubReq('GET', '/api/guest/session', fresh, undefined, ip)).status, 401)
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, ip)).status, 401)
      assert.equal((await hostReq('PATCH', '/api/share', { rotate: true })).status, 409)
    })
    it('share of a missing board is 404', async () => {
      assert.equal((await hostReq('POST', '/api/share', { boardId: 'nope' })).status, 404)
    })
    it('deleting the shared board ends the share', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      await hostReq('DELETE', `/api/boards/${id}`)
      assert.equal((await pubReq('GET', '/api/guest/session', s.code, undefined, { 'cf-connecting-ip': '198.51.100.50' })).status, 401)
    })
  })

  describe('sharing: WebSocket / Yjs', () => {
    const wsUrl = () => `ws://127.0.0.1:${pp}/collab`

    it('upgrade refused: no share, wrong Origin, missing Origin', async () => {
      assert.equal(await wsProbe(wsUrl(), { origin: TUNNEL }), 403)
      const id = await newBoard()
      await share(id, 'edit')
      assert.equal(await wsProbe(wsUrl(), { origin: 'https://evil.example.com' }), 403)
      assert.equal(await wsProbe(wsUrl(), {}), 403)
      assert.equal(await wsProbe(`ws://127.0.0.1:${pp}/other`, { origin: TUNNEL }), 403)
      assert.equal(await wsProbe(wsUrl(), { origin: TUNNEL }), 'open')
      await hostReq('DELETE', '/api/share')
    })

    it('wrong code, self-chosen id, pending/denied ticket and wrong board fail authentication', async () => {
      const id = await newBoard()
      const other = await newBoard()
      const s = await share(id, 'edit')
      const pend = (await joinReq(s.code, 'P')).json.ticket
      const den = (await joinReq(s.code, 'D')).json.ticket
      await admit(den, false)
      const ok = await admitted(s.code)
      assert.notEqual(await guest(id, `wrongcode.${ok.ticket}`).failed(), 'timeout')
      assert.notEqual(await guest(id, `${s.code}.selfchosen01`).failed(), 'timeout')
      assert.notEqual(await guest(id, `${s.code}.${pend}`).failed(), 'timeout')
      assert.notEqual(await guest(id, `${s.code}.${den}`).failed(), 'timeout')
      assert.notEqual(await guest(other, ok.token).failed(), 'timeout') // admitted, but not the shared board
      assert.equal(await guest(id, ok.token).authenticated(), 'read-write')
      await hostReq('DELETE', '/api/share')
    })

    it('edit guest: change reaches the host doc, shows online; kick closes it and blocks reconnect', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      const a = await admitted(s.code, 'Zed')
      const g = guest(id, a.token)
      assert.equal(await g.authenticated(), 'read-write')
      g.provider.document.getMap('m').set('fromGuest', 'yes')
      assert.ok(await until(async () => (await exportMap(id)).fromGuest === 'yes'), 'guest change must show in export')
      assert.ok(await until(async () => (await hostReq('GET', '/api/share')).json.guests.some((x: any) => x.ticket === a.ticket && x.online)))
      await hostReq('POST', '/api/share/kick', { ticket: a.ticket })
      assert.ok(await until(() => (g.provider.configuration.websocketProvider as any).status !== 'connected'), 'kicked socket must close')
      assert.notEqual(await guest(id, a.token).failed(), 'timeout')
      await hostReq('DELETE', '/api/share')
    })

    it('autoAdmit guest can connect right after join', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit', { autoAdmit: true })
      const j = (await joinReq(s.code, 'Auto')).json
      assert.equal(await guest(id, `${s.code}.${j.ticket}`).authenticated(), 'read-write')
      await hostReq('DELETE', '/api/share')
    })

    it('view-only guest: authenticated as readonly, its change is NOT persisted', async () => {
      const id = await newBoard()
      const s = await share(id, 'view')
      const a = await admitted(s.code)
      const g = guest(id, a.token)
      assert.equal(await g.authenticated(), 'readonly')
      g.provider.document.getMap('m').set('fromGuest', 'nope')
      await new Promise((r) => setTimeout(r, 400))
      assert.equal((await exportMap(id)).fromGuest, undefined)
      await hostReq('DELETE', '/api/share')
    })

    it('rotate closes live guests and their old ticket no longer authenticates', async () => {
      const id = await newBoard()
      const s = await share(id, 'edit')
      const a = await admitted(s.code)
      const g = guest(id, a.token)
      assert.equal(await g.authenticated(), 'read-write')
      await hostReq('PATCH', '/api/share', { rotate: true })
      assert.ok(await until(() => (g.provider.configuration.websocketProvider as any).status !== 'connected'), 'guest socket must be closed')
      assert.notEqual(await guest(id, a.token).failed(), 'timeout')
      await hostReq('DELETE', '/api/share')
    })
  })
})
