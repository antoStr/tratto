import { safeColor } from './doc.ts'
import { shapePolygon } from './geometry.ts'
import { outlineToPath, strokeOutline } from './ink.ts'
import { stampBitmap } from './stamps.ts'
import { FONT_STACK, type BoardMeta, type Camera, type El, type FontKind, type InkEl, type LineEl, type ShapeEl, type StickyEl, type TextEl } from './types.ts'

export const LINE_HEIGHT = 1.3
export const STICKY_PAD = 16

/* ---------------- colours ---------------- */

export function isDark(hex: string) {
  const m = /^#?([0-9a-f]{6})/i.exec(hex)
  if (!m) return false
  const n = parseInt(m[1], 16)
  const lum = (0.299 * (n >> 16) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255
  return lum < 0.5
}

/* ---------------- text layout ---------------- */

let measure: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null = null
function ctx2d() {
  measure ??= typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(1, 1).getContext('2d')! : document.createElement('canvas').getContext('2d')!
  return measure
}

export const fontCss = (font: FontKind, size: number, bold = false, italic = false) =>
  `${italic ? 'italic ' : ''}${bold ? 650 : font === 'hand' ? 500 : 400} ${size}px ${FONT_STACK[font] ?? FONT_STACK.sans}`

export interface TextLayout {
  lines: string[]
  width: number
  height: number
}

/** Splits text into lines, wrapping words at `maxWidth` (null = only at newlines). */
export function layoutText(text: string, css: string, size: number, maxWidth: number | null): TextLayout {
  const c = ctx2d()
  c.font = css
  const lines: string[] = []
  let width = 0
  const push = (line: string) => {
    lines.push(line)
    width = Math.max(width, c.measureText(line).width)
  }
  for (const para of text.split('\n')) {
    if (maxWidth === null) {
      push(para)
      continue
    }
    let line = ''
    for (const word of para.split(/(?<=\s)/)) {
      const next = line + word
      if (c.measureText(next.trimEnd()).width <= maxWidth || !line) {
        line = next
        // A single word wider than the box is broken by characters.
        while (c.measureText(line.trimEnd()).width > maxWidth && line.length > 1) {
          let cut = line.length - 1
          while (cut > 1 && c.measureText(line.slice(0, cut)).width > maxWidth) cut--
          push(line.slice(0, cut))
          line = line.slice(cut)
        }
      } else {
        push(line.trimEnd())
        line = word
      }
    }
    push(line.trimEnd())
  }
  return { lines, width, height: lines.length * size * LINE_HEIGHT }
}

let layoutCache = new WeakMap<El, { layout: TextLayout; size: number }>()
/** Call when web fonts finish loading: cached measurements used the fallback font. */
export function resetTextCaches() {
  layoutCache = new WeakMap()
}

export function textLayout(el: TextEl): TextLayout {
  const hit = layoutCache.get(el)
  if (hit) return hit.layout
  const layout = layoutText(el.text || ' ', fontCss(el.font, el.fontSize, el.bold, el.italic), el.fontSize, el.fixedWidth ? el.w : null)
  layoutCache.set(el, { layout, size: el.fontSize })
  return layout
}

/** Largest font size (8–28) at which the sticky's text fits its square. */
export function stickyLayout(el: StickyEl): { layout: TextLayout; size: number } {
  const hit = layoutCache.get(el)
  if (hit) return hit
  const maxW = Math.max(10, el.w - STICKY_PAD * 2)
  const maxH = Math.max(10, el.h - STICKY_PAD * 2)
  let size = 28
  let layout = layoutText(el.text, fontCss(el.font, size), size, maxW)
  while (size > 8 && (layout.height > maxH || layout.width > maxW + 0.5)) {
    size = Math.max(8, Math.floor(size * 0.9))
    layout = layoutText(el.text, fontCss(el.font, size), size, maxW)
  }
  const res = { layout, size }
  layoutCache.set(el, res)
  return res
}

/* ---------------- paths ---------------- */

const pathCache = new WeakMap<El, Path2D>()

export function inkPathData(el: InkEl) {
  return outlineToPath(strokeOutline(el.points, el.size, el.type === 'highlighter'))
}

function inkPath(el: InkEl) {
  let p = pathCache.get(el)
  if (!p) {
    p = new Path2D(inkPathData(el))
    pathCache.set(el, p)
  }
  return p
}

export function shapePath(el: ShapeEl): Path2D {
  let p = pathCache.get(el)
  if (p) return p
  p = new Path2D()
  if (el.shape === 'rect') p.roundRect(0, 0, el.w, el.h, Math.min(el.radius, el.w / 2, el.h / 2))
  else if (el.shape === 'ellipse') p.ellipse(el.w / 2, el.h / 2, el.w / 2, el.h / 2, 0, 0, Math.PI * 2)
  else {
    const poly = shapePolygon(el.shape, el.w, el.h, el.points) ?? []
    for (let i = 0; i < poly.length; i += 2) (i ? p.lineTo : p.moveTo).call(p, poly[i], poly[i + 1])
    p.closePath()
  }
  pathCache.set(el, p)
  return p
}

/** Arrow head triangle at (x2, y2) pointing away from (x1, y1). */
export function arrowHead(x1: number, y1: number, x2: number, y2: number, width: number): number[] {
  const len = Math.max(10, width * 3.6)
  const a = Math.atan2(y2 - y1, x2 - x1)
  const spread = Math.PI / 7
  return [x2, y2, x2 - len * Math.cos(a - spread), y2 - len * Math.sin(a - spread), x2 - len * Math.cos(a + spread), y2 - len * Math.sin(a + spread)]
}

/** Where the line body should end so it doesn't poke through the arrow tip. */
export function lineEnds(el: LineEl) {
  const [x1, y1, x2, y2] = el.points
  const len = Math.hypot(x2 - x1, y2 - y1) || 1
  const back = Math.max(10, el.strokeWidth * 3.6) * 0.6
  const ux = (x2 - x1) / len
  const uy = (y2 - y1) / len
  return {
    sx: el.arrowStart ? x1 + ux * back : x1,
    sy: el.arrowStart ? y1 + uy * back : y1,
    ex: el.arrowEnd ? x2 - ux * back : x2,
    ey: el.arrowEnd ? y2 - uy * back : y2,
  }
}

/* ---------------- images ---------------- */

export class ImageStore {
  private items = new Map<string, { bitmap: ImageBitmap | null; failed: boolean }>()
  private fetchBlob: (fileId: string) => Promise<Blob>
  private onLoad: () => void
  constructor(fetchBlob: (fileId: string) => Promise<Blob>, onLoad: () => void) {
    this.fetchBlob = fetchBlob
    this.onLoad = onLoad
  }

  get(fileId: string): ImageBitmap | null | 'failed' {
    const item = this.items.get(fileId)
    if (item) return item.failed ? 'failed' : item.bitmap
    const entry = { bitmap: null as ImageBitmap | null, failed: false }
    this.items.set(fileId, entry)
    this.fetchBlob(fileId)
      .then((b) => createImageBitmap(b))
      .then((bmp) => (entry.bitmap = bmp))
      .catch(() => (entry.failed = true))
      .finally(this.onLoad)
    return null
  }

  /** Puts a just-uploaded image in the cache so it shows without a round trip. */
  prime(fileId: string, bitmap: ImageBitmap) {
    this.items.set(fileId, { bitmap, failed: false })
  }
}

/* ---------------- drawing ---------------- */

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D

export interface DrawEnv {
  images: ImageStore | null
  /** Text of this element is being edited in a DOM overlay. */
  editingId?: string | null
  /** Pre-resolved bitmaps for export. */
  bitmaps?: Map<string, ImageBitmap>
  /** Current zoom; on screen, very thin strokes are drawn at least one pixel wide. */
  zoom?: number
}

export function drawElement(ctx: Ctx, el: El, env: DrawEnv) {
  if (el.hidden) return
  ctx.save()
  ctx.globalAlpha = Math.max(0, Math.min(1, el.opacity))
  ctx.translate(el.x + el.w / 2, el.y + el.h / 2)
  if (el.rotation) ctx.rotate(el.rotation)
  ctx.translate(-el.w / 2, -el.h / 2)

  switch (el.type) {
    case 'ink':
    case 'highlighter':
      if (el.type === 'highlighter') ctx.globalAlpha *= 0.45
      ctx.fillStyle = safeColor(el.color)
      ctx.fill(inkPath(el))
      // Zoomed far out a stroke can shrink below a pixel and vanish: keep a hairline.
      if (env.zoom && el.size * env.zoom < 1) {
        ctx.strokeStyle = ctx.fillStyle
        ctx.lineWidth = 1 / env.zoom
        ctx.lineJoin = 'round'
        ctx.stroke(inkPath(el))
      }
      break
    case 'shape': {
      const path = shapePath(el)
      if (el.fill !== 'transparent') {
        ctx.fillStyle = safeColor(el.fill)
        ctx.fill(path)
      }
      if (el.strokeWidth > 0 && el.stroke !== 'transparent') {
        ctx.strokeStyle = safeColor(el.stroke)
        ctx.lineWidth = el.strokeWidth
        ctx.lineJoin = 'round'
        if (el.dash) ctx.setLineDash([el.strokeWidth * 3, el.strokeWidth * 2.2])
        ctx.stroke(path)
      }
      break
    }
    case 'line': {
      const { sx, sy, ex, ey } = lineEnds(el)
      const color = safeColor(el.stroke)
      ctx.strokeStyle = color
      ctx.fillStyle = color
      ctx.lineWidth = el.strokeWidth
      ctx.lineCap = 'round'
      if (el.dash) ctx.setLineDash([el.strokeWidth * 3, el.strokeWidth * 2.2])
      ctx.beginPath()
      ctx.moveTo(sx, sy)
      ctx.lineTo(ex, ey)
      ctx.stroke()
      ctx.setLineDash([])
      const [x1, y1, x2, y2] = el.points
      for (const head of [el.arrowEnd && arrowHead(x1, y1, x2, y2, el.strokeWidth), el.arrowStart && arrowHead(x2, y2, x1, y1, el.strokeWidth)]) {
        if (!head) continue
        ctx.beginPath()
        ctx.moveTo(head[0], head[1])
        ctx.lineTo(head[2], head[3])
        ctx.lineTo(head[4], head[5])
        ctx.closePath()
        ctx.lineJoin = 'round'
        ctx.lineWidth = Math.max(1, el.strokeWidth * 0.5)
        ctx.fill()
        ctx.stroke()
      }
      break
    }
    case 'text': {
      if (env.editingId === el.id) break
      const layout = textLayout(el)
      ctx.font = fontCss(el.font, el.fontSize, el.bold, el.italic)
      ctx.fillStyle = safeColor(el.color)
      ctx.textBaseline = 'middle'
      drawLines(ctx, layout.lines, el.align, el.w, 0, el.fontSize)
      break
    }
    case 'sticky': {
      ctx.save()
      ctx.shadowColor = 'rgba(0,0,0,0.14)'
      ctx.shadowBlur = 10
      ctx.shadowOffsetY = 3
      ctx.fillStyle = safeColor(el.color, '#FFF3A3')
      ctx.beginPath()
      ctx.roundRect(0, 0, el.w, el.h, 4)
      ctx.fill()
      ctx.restore()
      if (env.editingId === el.id || !el.text) break
      const { layout, size } = stickyLayout(el)
      ctx.font = fontCss(el.font, size)
      ctx.fillStyle = isDark(el.color) ? '#FFFFFF' : '#1E1E1E'
      ctx.textBaseline = 'middle'
      const top = (el.h - layout.height) / 2
      drawLines(ctx, layout.lines, el.align, el.w - STICKY_PAD * 2, top, size, STICKY_PAD)
      break
    }
    case 'image': {
      const bmp = env.bitmaps?.get(el.fileId) ?? env.images?.get(el.fileId) ?? null
      if (bmp && bmp !== 'failed') {
        ctx.drawImage(bmp, 0, 0, el.w, el.h)
      } else {
        ctx.fillStyle = bmp === 'failed' ? 'rgba(242,72,34,0.08)' : 'rgba(128,128,128,0.12)'
        ctx.fillRect(0, 0, el.w, el.h)
        ctx.strokeStyle = 'rgba(128,128,128,0.35)'
        ctx.setLineDash([6, 6])
        ctx.strokeRect(0.5, 0.5, el.w - 1, el.h - 1)
      }
      break
    }
    case 'stamp': {
      const bmp = stampBitmap(el.emoji)
      if (bmp) {
        ctx.drawImage(bmp, 0, 0, el.w, el.h)
        break
      }
      ctx.font = `${el.h * 0.86}px "Segoe UI Emoji", "Apple Color Emoji", "Noto Color Emoji", sans-serif`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillText(el.emoji, el.w / 2, el.h / 2 + el.h * 0.04)
      break
    }
  }
  ctx.restore()
}

function drawLines(ctx: Ctx, lines: string[], align: string, width: number, top: number, size: number, left = 0) {
  ctx.textAlign = align === 'center' ? 'center' : align === 'right' ? 'right' : 'left'
  const x = left + (align === 'center' ? width / 2 : align === 'right' ? width : 0)
  lines.forEach((line, i) => ctx.fillText(line, x, top + size * LINE_HEIGHT * (i + 0.5)))
}

/** Board colour plus a pattern (dots, grid, lines, graph paper, isometric) readable at any zoom. */
export function drawBackground(ctx: Ctx, cam: Camera, width: number, height: number, meta: BoardMeta) {
  ctx.fillStyle = meta.background
  ctx.fillRect(0, 0, width, height)
  if (meta.pattern === 'none') return
  // Pattern spacing on screen, kept between 14 and ~100 px by doubling/halving.
  let k = 1
  while (meta.gridSize * cam.z * k < 14) k *= 2
  while (meta.gridSize * cam.z * k > 100) k /= 2
  const step = meta.gridSize * cam.z * k
  const dark = isDark(meta.background)
  const ink = (a: number) => (dark ? `rgba(255,255,255,${a})` : `rgba(0,0,0,${a})`)
  const ox = ((cam.x % step) + step) % step
  const oy = ((cam.y % step) + step) % step
  const lines = (s: number, x0: number, y0: number, vertical: boolean) => {
    ctx.beginPath()
    if (vertical)
      for (let x = x0; x < width; x += s) {
        ctx.moveTo(Math.round(x) + 0.5, 0)
        ctx.lineTo(Math.round(x) + 0.5, height)
      }
    for (let y = y0; y < height; y += s) {
      ctx.moveTo(0, Math.round(y) + 0.5)
      ctx.lineTo(width, Math.round(y) + 0.5)
    }
    ctx.stroke()
  }
  ctx.lineWidth = 1
  switch (meta.pattern) {
    case 'dots': {
      ctx.fillStyle = ink(0.16)
      const r = 1.1
      for (let x = ox; x < width; x += step) for (let y = oy; y < height; y += step) ctx.fillRect(x - r, y - r, r * 2, r * 2)
      return
    }
    case 'isometric': {
      // Dots on a triangular lattice: every other row is shifted by half a step.
      ctx.fillStyle = ink(0.18)
      const r = 1.1
      const rowH = step * 0.866
      const wy = cam.y / rowH
      const firstRow = Math.floor(-wy) - 1
      for (let row = firstRow; row * rowH + cam.y < height + rowH; row++) {
        const y = row * rowH + cam.y
        const shift = (((row % 2) + 2) % 2) * (step / 2)
        const x0 = ((((cam.x + shift) % step) + step) % step)
        for (let x = x0; x < width; x += step) ctx.fillRect(x - r, y - r, r * 2, r * 2)
      }
      return
    }
    case 'graph': {
      // Graph paper: fine lines plus a stronger line every five squares.
      const major = step * 5
      ctx.strokeStyle = ink(0.05)
      lines(step, ox, oy, true)
      ctx.strokeStyle = ink(0.11)
      lines(major, ((cam.x % major) + major) % major, ((cam.y % major) + major) % major, true)
      return
    }
    case 'grid':
      ctx.strokeStyle = ink(0.07)
      lines(step, ox, oy, true)
      return
    case 'lines':
      ctx.strokeStyle = ink(0.08)
      lines(step, ox, oy, false)
      return
  }
}
