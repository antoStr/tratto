// Sandboxed preloads must be CommonJS.
const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('tratto', {
  desktop: true,
  platform: process.platform,
  setTheme: (theme) => ipcRenderer.send('tratto:theme', theme),
})
