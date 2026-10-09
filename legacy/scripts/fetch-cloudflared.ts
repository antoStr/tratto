// Downloads the pinned cloudflared binary and verifies its SHA256.
// Usage: node scripts/fetch-cloudflared.ts [win|linux|mac-x64|mac-arm64]   (default: current platform)
// Checksums are the ones Cloudflare publishes in the release notes.
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, writeFileSync, chmodSync, existsSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const VERSION = '2026.9.3'
const TARGETS = {
  win: { asset: 'cloudflared-windows-amd64.exe', out: 'bin/cloudflared.exe', sha: 'f096265ec2fcbe9bb6e2d64268db167ced3fcbb83d894bdb9e2fcdb26f2ea7e2' },
  linux: { asset: 'cloudflared-linux-amd64', out: 'bin/cloudflared', sha: '77e26d8d900e0b8469f416239d14b5f296525fdf79fee6f511ef55609e3fbac2' },
  // macOS ships as a .tgz; `out` follows electron-builder's ${arch} names (see electron-builder.yml).
  'mac-x64': { asset: 'cloudflared-darwin-amd64.tgz', out: 'bin/cloudflared-darwin-x64', sha: 'ab588b3b4db9cdb4476c30a3db2a72635b1d8327d44741fee6799a0f37b0ec07' },
  'mac-arm64': { asset: 'cloudflared-darwin-arm64.tgz', out: 'bin/cloudflared-darwin-arm64', sha: '5472c1a01c84bc31b3021056a73b4e5774ddddefc572124ea8fdf6c340639f32' },
}

const here = process.platform === 'win32' ? 'win' : process.platform === 'darwin' ? `mac-${process.arch}` : 'linux'
const key = (process.argv[2] ?? here) as keyof typeof TARGETS
const target = TARGETS[key]
if (!target) throw new Error(`Unknown target "${key}", use ${Object.keys(TARGETS).join(', ')}`)
const archive = target.asset.endsWith('.tgz')

const sha256 = (buf: Uint8Array) => createHash('sha256').update(buf).digest('hex')

if (existsSync(target.out) && sha256(readFileSync(target.out)) === target.sha) {
  console.log(`${target.out} already at ${VERSION}`)
} else {
  const url = `https://github.com/cloudflare/cloudflared/releases/download/${VERSION}/${target.asset}`
  console.log(`Downloading ${url}`)
  const res = await fetch(url)
  if (!res.ok) throw new Error(`Download failed: HTTP ${res.status}`)
  let buf = new Uint8Array(await res.arrayBuffer())
  if (archive) {
    const dir = mkdtempSync(join(tmpdir(), 'cloudflared-'))
    try {
      writeFileSync(join(dir, 'a.tgz'), buf)
      execFileSync('tar', ['-xzf', join(dir, 'a.tgz'), '-C', dir, 'cloudflared'])
      buf = readFileSync(join(dir, 'cloudflared'))
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  }
  // Cloudflare's published checksums are of the binary itself, also for the macOS archives.
  const got = sha256(buf)
  if (got !== target.sha) throw new Error(`Checksum mismatch for ${target.asset}: ${got}`)
  mkdirSync('bin', { recursive: true })
  writeFileSync(target.out, buf)
  chmodSync(target.out, 0o755)
  console.log(`Saved ${target.out} (sha256 ok)`)
}
