// Downloads the pinned cloudflared binary and verifies its SHA256.
// Usage: node scripts/fetch-cloudflared.ts [win|linux]   (default: current platform)
import { createHash } from 'node:crypto'
import { mkdirSync, writeFileSync, chmodSync, existsSync, readFileSync } from 'node:fs'

const VERSION = '2026.9.3'
const TARGETS = {
  win: { asset: 'cloudflared-windows-amd64.exe', out: 'bin/cloudflared.exe', sha: 'f096265ec2fcbe9bb6e2d64268db167ced3fcbb83d894bdb9e2fcdb26f2ea7e2' },
  linux: { asset: 'cloudflared-linux-amd64', out: 'bin/cloudflared', sha: '77e26d8d900e0b8469f416239d14b5f296525fdf79fee6f511ef55609e3fbac2' },
}

const key = (process.argv[2] ?? (process.platform === 'win32' ? 'win' : 'linux')) as keyof typeof TARGETS
const target = TARGETS[key]
if (!target) throw new Error(`Unknown target "${key}", use win or linux`)

const sha256 = (buf: Uint8Array) => createHash('sha256').update(buf).digest('hex')

if (existsSync(target.out) && sha256(readFileSync(target.out)) === target.sha) {
  console.log(`${target.out} already at ${VERSION}`)
} else {
  const url = `https://github.com/cloudflare/cloudflared/releases/download/${VERSION}/${target.asset}`
  console.log(`Downloading ${url}`)
  const res = await fetch(url)
  if (!res.ok) throw new Error(`Download failed: HTTP ${res.status}`)
  const buf = new Uint8Array(await res.arrayBuffer())
  const got = sha256(buf)
  if (got !== target.sha) throw new Error(`Checksum mismatch for ${target.asset}: ${got}`)
  mkdirSync('bin', { recursive: true })
  writeFileSync(target.out, buf)
  chmodSync(target.out, 0o755)
  console.log(`Saved ${target.out} (sha256 ok)`)
}
