// electron-builder afterPack hook. macOS without a paid Apple certificate: sign the app ad hoc.
// Unsigned, Apple Silicon Macs call the downloaded app "damaged"; ad hoc signed, they only ask
// to confirm the first launch (System Settings › Privacy & Security › Open Anyway).
const { execFileSync } = require('node:child_process')
const { join } = require('node:path')

exports.default = async function adhocSign(ctx) {
  if (ctx.electronPlatformName !== 'darwin') return
  const app = join(ctx.appOutDir, `${ctx.packager.appInfo.productFilename}.app`)
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', app], { stdio: 'inherit' })
}
