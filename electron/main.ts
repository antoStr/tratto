import { app, BrowserWindow, Menu, nativeTheme, ipcMain, session, shell } from 'electron'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { startServers } from '../server/index.ts'

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
  win.once('ready-to-show', () => win?.show())
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
