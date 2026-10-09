// Sandboxed preloads must be CommonJS.
const { contextBridge, ipcRenderer } = require('electron')

// Keep in sync with TrattoBridge in src/desktop.ts.
contextBridge.exposeInMainWorld('tratto', {
  desktop: true,
  platform: process.platform,
  setTheme: (theme) => ipcRenderer.send('tratto:theme', theme),
  setZoom: (factor) => ipcRenderer.send('tratto:zoom', factor),
  getInfo: () => ipcRenderer.invoke('tratto:info'),
  getUpdateState: () => ipcRenderer.invoke('tratto:update-state'),
  checkForUpdates: () => ipcRenderer.invoke('tratto:update-check'),
  installUpdate: () => ipcRenderer.send('tratto:update-install'),
  onUpdate: (cb) => {
    const h = (_e, state) => cb(state)
    ipcRenderer.on('tratto:update', h)
    return () => ipcRenderer.removeListener('tratto:update', h)
  },
})
