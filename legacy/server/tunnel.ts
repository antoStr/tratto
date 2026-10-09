import { spawn, type ChildProcess } from 'node:child_process'

export interface Tunnel {
  url: string
  stop(): void
}

/**
 * Opens a Cloudflare quick tunnel to a local port. The PC only makes an outbound
 * connection to Cloudflare, so no router port is opened and guests never see its IP.
 */
export function openTunnel(binary: string, port: number, onExit: (reason: string) => void): Promise<Tunnel> {
  return new Promise((resolve, reject) => {
    let proc: ChildProcess
    try {
      proc = spawn(binary, ['tunnel', '--no-autoupdate', '--url', `http://127.0.0.1:${port}`], {
        stdio: ['ignore', 'ignore', 'pipe'],
        windowsHide: true,
      })
    } catch (e) {
      return reject(e)
    }
    let url: string | null = null
    let ready = false
    let log = ''
    const fail = (msg: string) => {
      clearTimeout(timer)
      proc.kill()
      reject(new Error(msg))
    }
    const timer = setTimeout(() => fail('Cloudflare non risponde da 45 secondi. Controlla la connessione a internet.'), 45_000)

    proc.on('error', (e) => fail(`Impossibile avviare la condivisione: ${e.message}`))
    proc.stderr!.setEncoding('utf8')
    proc.stderr!.on('data', (chunk: string) => {
      log = (log + chunk).slice(-4000)
      url ??= chunk.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/)?.[0] ?? null
      if (!ready && url && /Registered tunnel connection/.test(log)) {
        ready = true
        clearTimeout(timer)
        resolve({ url, stop: () => proc.kill() })
      }
    })
    proc.on('exit', (code) => {
      if (!ready) return fail(`La condivisione si è chiusa (codice ${code}). ${log.split('\n').filter((l) => /ERR/.test(l)).slice(-1)[0] ?? ''}`)
      onExit(`cloudflared exited (${code})`)
    })
  })
}
