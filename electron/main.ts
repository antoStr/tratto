import { app, BrowserWindow, Menu, nativeTheme, ipcMain, session, shell } from 'electron'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import updater from 'electron-updater'
import { startServers } from '../server/index.ts'
import type { UpdateState } from '../src/desktop.ts'

// electron-updater is CommonJS: take the singleton from the default export.
const { autoUpdater } = updater

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), '..')
const exe = process.platform === 'win32' ? 'cloudflared.exe' : 'cloudflared'

const ALLOWED_PERMISSIONS = new Set(['clipboard-sanitized-write', 'clipboard-read', 'fullscreen'])
const THEMES = ['system', 'light', 'dark']

if (process.platform === 'win32') app.setAppUserModelId('app.tratto.desktop')

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  main().catch((err) => {
    console.error(err)
    app.exit(1)
  })
}

/** The tab bar is dark in both themes, like Figma's desktop app, so the window buttons match it. */
function overlay() {
  return { height: 40, color: '#2C2C2C', symbolColor: '#FFFFFF' }
}

async function main() {
  let win: BrowserWindow | null = null
  let servers: Awaited<ReturnType<typeof startServers>> | null = null

  app.on('second-instance', () => {
    if (!win) return
    if (win.isMinimized()) win.restore()
    win.focus()
  })
  app.on('window-all-closed', () => app.quit())

  let stopping = false
  app.on('before-quit', (e) => {
    if (stopping || !servers) return
    e.preventDefault()
    stopping = true
    servers.stop().catch((err) => console.error('stop failed', err)).finally(() => app.quit())
  })

  // ---- Auto update (GitHub Releases). Only meaningful in the installed app. ----
  let update: UpdateState = app.isPackaged ? { state: 'idle' } : { state: 'unsupported' }
  const setUpdate = (s: UpdateState) => {
    update = s
    if (win && !win.isDestroyed()) win.webContents.send('tratto:update', s)
  }
  const busy = () => update.state === 'checking' || update.state === 'downloading' || update.state === 'ready'
  async function checkForUpdates(): Promise<UpdateState> {
    if (!app.isPackaged || busy()) return update
    try {
      await autoUpdater.checkForUpdates() // events below drive the state
    } catch {
      // already reported through the 'error' event
    }
    return update
  }
  if (app.isPackaged) {
    autoUpdater.autoDownload = true
    autoUpdater.autoInstallOnAppQuit = true // installs on a normal quit, after before-quit flushed the boards
    autoUpdater.on('checking-for-update', () => setUpdate({ state: 'checking' }))
    autoUpdater.on('update-not-available', () => setUpdate({ state: 'latest', checkedAt: Date.now() }))
    autoUpdater.on('update-available', (i) => setUpdate({ state: 'downloading', version: i.version, percent: 0 }))
    autoUpdater.on('download-progress', (p) => {
      if (update.state === 'downloading') setUpdate({ ...update, percent: Math.round(p.percent) })
    })
    autoUpdater.on('update-downloaded', (i) => setUpdate({ state: 'ready', version: i.version }))
    autoUpdater.on('error', (err) => {
      console.error('updater', err)
      setUpdate({ state: 'error', message: 'Impossibile controllare gli aggiornamenti. Controlla la connessione e riprova.' })
    })
  }

  await app.whenReady()
  Menu.setApplicationMenu(null)

  servers = await startServers({
    dataDir: app.getPath('userData'),
    staticDir: app.isPackaged ? join(app.getAppPath(), 'dist') : process.env.TRATTO_DEV ? null : join(projectRoot, 'dist'),
    cloudflared: app.isPackaged ? join(process.resourcesPath, exe) : join(projectRoot, 'bin', exe),
    hostPort: 47615,
  })
  const { hostUrl, hostOrigin } = servers

  const ses = session.defaultSession
  ses.setPermissionRequestHandler((_wc, permission, cb) => cb(ALLOWED_PERMISSIONS.has(permission)))
  ses.setPermissionCheckHandler((_wc, permission) => ALLOWED_PERMISSIONS.has(permission))

  const applyTheme = () => {
    if (!win || win.isDestroyed()) return
    win.setBackgroundColor(nativeTheme.shouldUseDarkColors ? '#1E1E1E' : '#F5F5F5')
    win.setTitleBarOverlay(overlay())
  }
  ipcMain.on('tratto:theme', (e, theme) => {
    if (e.senderFrame?.url.startsWith(hostOrigin) && THEMES.includes(theme)) nativeTheme.themeSource = theme
  })
  nativeTheme.on('updated', applyTheme)

  const fromHost = (e: Electron.IpcMainEvent | Electron.IpcMainInvokeEvent) => !!e.senderFrame?.url.startsWith(hostOrigin)
  ipcMain.handle('tratto:info', (e) => {
    if (!fromHost(e)) throw new Error('forbidden')
    return { version: app.getVersion() }
  })
  ipcMain.handle('tratto:update-state', (e) => {
    if (!fromHost(e)) throw new Error('forbidden')
    return update
  })
  ipcMain.handle('tratto:update-check', (e) => {
    if (!fromHost(e)) throw new Error('forbidden')
    return checkForUpdates()
  })
  ipcMain.on('tratto:update-install', async (e) => {
    if (!fromHost(e) || update.state !== 'ready' || !servers) return
    // quitAndInstall spawns the installer immediately (before the app quits), and the installer
    // closes/kills a running Tratto. So flush boards and stop the tunnel first, then install.
    stopping = true
    await servers.stop().catch((err) => console.error('stop failed', err))
    autoUpdater.quitAndInstall(true, true)
    // On failure quitAndInstall resets this flag and does not quit: quit ourselves (servers are down).
    if (!(autoUpdater as unknown as { quitAndInstallCalled: boolean }).quitAndInstallCalled) app.quit()
  })
  ipcMain.on('tratto:zoom', (e, factor) => {
    if (fromHost(e) && typeof factor === 'number' && factor >= 0.75 && factor <= 2) win?.webContents.setZoomFactor(factor)
  })

  win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 960,
    minHeight: 640,
    show: false,
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#1E1E1E' : '#F5F5F5',
    title: 'Tratto',
    titleBarStyle: 'hidden',
    titleBarOverlay: overlay(),
    webPreferences: {
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webviewTag: false,
      spellcheck: false,
      preload: join(import.meta.dirname, 'preload.cjs'),
    },
  })
  win.once('ready-to-show', () => {
    win?.show()
    if (app.isPackaged) {
      setTimeout(() => void checkForUpdates(), 10_000)
      setInterval(() => void checkForUpdates(), 4 * 3600_000)
    }
  })
  win.on('closed', () => (win = null))

  const wc = win.webContents
  wc.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https:')) void shell.openExternal(url)
    return { action: 'deny' }
  })
  wc.on('will-navigate', (e, url) => {
    if (new URL(url).origin !== hostOrigin) e.preventDefault()
  })
  wc.on('will-attach-webview', (e) => e.preventDefault())
  if (!app.isPackaged) {
    wc.on('before-input-event', (e, input) => {
      if (input.type !== 'keyDown') return
      if (input.key === 'F12' || (input.control && input.shift && input.key.toLowerCase() === 'i')) {
        wc.toggleDevTools()
        e.preventDefault()
      }
    })
  }

  await win.loadURL(hostUrl)
}
