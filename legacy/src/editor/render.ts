import { safeColor } from './doc.ts'
import { aabb, shapePolygon } from './geometry.ts'
import { outlineToPath, strokeOutline } from './ink.ts'
import { stampBitmap } from './stamps.ts'
import { FONT_STACK, type BoardMeta, type Camera, type El, type EraseMark, type FontKind, type InkEl, type LineEl, type SectionEl, type ShapeEl, type StickyEl, type TextEl } from './types.ts'

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

const requestedFonts = new Set<string>()
/** Canvas text may not make the browser fetch a web font: ask for it (the board repaints on 'loadingdone'). */
function loadFont(css: string) {
  const key = css.replace(/[\d.]+px /, '')
  if (requestedFonts.has(key) || typeof document === 'undefined' || !document.fonts) return
  requestedFonts.add(key)
  document.fonts.load(css).catch(() => {})
}

/** Splits text into lines, wrapping words at `maxWidth` (null = only at newlines). */
export function layoutText(text: string, css: string, size: number, maxWidth: number | null): TextLayout {
  loadFont(css)
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

/** `patch` plus the box the text then needs: a self-sizing box fits the text, a fixed-width one only grows taller. */
export function fitText(el: TextEl, patch: Partial<TextEl>): Partial<TextEl> {
  const next = { ...el, ...patch }
  const l = layoutText(next.text || ' ', fontCss(next.font, next.fontSize, next.bold, next.italic), next.fontSize, next.fixedWidth ? next.w : null)
  // Room for the caret in an empty box, relative to the type (a fixed minimum is huge when zoomed in).
  return { ...patch, h: l.height, ...(next.fixedWidth ? {} : { w: Math.max(l.width, next.fontSize / 6) }) }
}

export function textLayout(el: TextEl): TextLayout {
  const hit = layoutCache.get(el)
  if (hit) return hit.layout
  const layout = layoutText(el.text || ' ', fontCss(el.font, el.fontSize, el.bold, el.italic), el.fontSize, el.fixedWidth ? el.w : null)
  layoutCache.set(el, { layout, size: el.fontSize })
  return layout
}

export const showAuthor = (el: StickyEl) => !!el.author && !el.hideAuthor
/** Height kept free at the bottom of a sticky for its author's name. */
export const authorBand = (el: StickyEl) => (showAuthor(el) ? el.h * 0.12 : 0)

/** Largest font size (8–28) at which the sticky's text fits its square. */
export function stickyLayout(el: StickyEl): { layout: TextLayout; size: number } {
  const hit = layoutCache.get(el)
  if (hit) return hit
  const maxW = Math.max(10, el.w - STICKY_PAD * 2)
  const maxH = Math.max(10, el.h - STICKY_PAD * 2 - authorBand(el))
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

/** How much of a shape's box its text may use, and how far the text sits below the centre (both × size). */
const SHAPE_TEXT: Partial<Record<ShapeEl['shape'], [number, number]>> = {
  ellipse: [0.72, 0],
  diamond: [0.55, 0],
  triangle: [0.5, 0.17],
  triangleDown: [0.5, -0.17],
  star: [0.42, 0.06],
  plus: [0.34, 0],
  pentagon: [0.68, 0.05],
  hexagon: [0.72, 0],
  octagon: [0.8, 0],
  parallelogram: [0.62, 0],
  arrowRight: [0.5, 0],
  arrowLeft: [0.5, 0],
}

/** Text inside a shape: largest size (up to 24) that fits its usable area, and where it starts. */
export function shapeTextLayout(el: ShapeEl): { layout: TextLayout; size: number; top: number; left: number; width: number } {
  const [k, shift] = SHAPE_TEXT[el.shape] ?? [0.86, 0]
  const maxW = Math.max(1, el.w * k)
  const maxH = Math.max(1, el.h * k)
  const font = el.font ?? 'sans'
  const start = Math.min(24, maxH)
  let size = start
  let layout = layoutText(el.text ?? '', fontCss(font, size), size, maxW)
  while (size > start * 0.15 && (layout.height > maxH || layout.width > maxW + 0.5)) {
    size *= 0.9
    layout = layoutText(el.text ?? '', fontCss(font, size), size, maxW)
  }
  return { layout, size, top: (el.h - layout.height) / 2 + el.h * shift, left: (el.w - maxW) / 2, width: maxW }
}

export function shapeTextColor(el: ShapeEl) {
  if (el.fill === 'transparent') return el.stroke === 'transparent' ? '#1E1E1E' : safeColor(el.stroke)
  return isDark(el.fill) ? '#FFFFFF' : '#1E1E1E'
}

/* ---------------- washi tape and comment pins ---------------- */

/** Size of a comment pin on screen, in px. */
export const COMMENT_PIN = 26

/** Washi tape: a translucent striped band with torn (zigzag) ends. */
function drawTape(ctx: Ctx, el: LineEl) {
  const [x1, y1, x2, y2] = el.points
  const len = Math.hypot(x2 - x1, y2 - y1)
  const w = el.strokeWidth
  const tooth = Math.min(w / 6, len / 4)
  ctx.save()
  ctx.translate(x1, y1)
  ctx.rotate(Math.atan2(y2 - y1, x2 - x1))
  const band = new Path2D()
  const teeth = Math.max(2, Math.round(w / (tooth * 2 || 1)))
  band.moveTo(0, -w / 2)
  band.lineTo(len, -w / 2)
  for (let i = 1; i <= teeth; i++) band.lineTo(len + (i % 2 ? -tooth : 0), -w / 2 + (w * i) / teeth)
  band.lineTo(0, w / 2)
  for (let i = 1; i <= teeth; i++) band.lineTo(i % 2 ? tooth : 0, w / 2 - (w * i) / teeth)
  band.closePath()
  ctx.globalAlpha *= 0.82
  ctx.fillStyle = safeColor(el.stroke)
  ctx.fill(band)
  // Diagonal stripes, lighter, clipped to the band.
  ctx.clip(band)
  ctx.strokeStyle = 'rgba(255,255,255,0.45)'
  ctx.lineWidth = w / 7
  ctx.beginPath()
  for (let x = -w; x < len + w; x += w / 2.5) {
    ctx.moveTo(x, w / 2)
    ctx.lineTo(x + w, -w / 2)
  }
  ctx.stroke()
  ctx.restore()
}

/** The tape's outline as a polygon (for SVG export), in the line's coordinates. */
export function tapeOutline(el: LineEl): number[] {
  const [x1, y1, x2, y2] = el.points
  const a = Math.atan2(y2 - y1, x2 - x1)
  const nx = -Math.sin(a) * (el.strokeWidth / 2)
  const ny = Math.cos(a) * (el.strokeWidth / 2)
  return [x1 - nx, y1 - ny, x2 - nx, y2 - ny, x2 + nx, y2 + ny, x1 + nx, y1 + ny]
}

/* ---------------- sections ---------------- */

const TITLE_FONT = (size: number) => `550 ${size}px "Inter Variable", "Segoe UI", system-ui, sans-serif`

/** The title pill above a section's top-left corner, in board units, at the given zoom (CSS px per unit). */
export function sectionTitleBox(el: SectionEl, zoom: number) {
  const size = 12 / zoom
  const pad = 8 / zoom
  const c = ctx2d()
  c.font = TITLE_FONT(size)
  const text = el.name?.trim() || 'Sezione'
  // A little slack: the board canvas measures text a hair wider than this one at some zooms.
  const w = Math.min(c.measureText(text).width + pad * 2 + size * 0.25, Math.max(el.w, pad * 4))
  const h = 22 / zoom
  return { x: el.x, y: el.y - h - 4 / zoom, w, h, size, pad, text }
}

function drawSectionTitle(ctx: Ctx, el: SectionEl, zoom: number, editing: boolean) {
  const t = sectionTitleBox(el, zoom)
  const x = t.x - el.x
  const y = t.y - el.y
  const fill = safeColor(el.fill, '#FFFFFF')
  ctx.fillStyle = fill
  ctx.beginPath()
  ctx.roundRect(x, y, t.w, t.h, 5 / zoom)
  ctx.fill()
  ctx.fillStyle = isDark(fill) ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.07)'
  ctx.fill()
  if (editing) return
  ctx.font = TITLE_FONT(t.size)
  ctx.fillStyle = isDark(fill) ? '#FFFFFF' : 'rgba(0,0,0,0.85)'
  ctx.textAlign = 'left'
  ctx.textBaseline = 'middle'
  let text = t.text
  const room = t.w - t.pad * 2 + t.size * 0.25
  if (ctx.measureText(text).width > room) {
    while (text.length > 1 && ctx.measureText(text + '…').width > room) text = text.slice(0, -1)
    text += '…'
  }
  ctx.fillText(text, x + t.pad, y + t.h / 2)
}

/* ---------------- paths ---------------- */

const pathCache = new WeakMap<El, Path2D>()

/** SVG path of a stroke, precise enough for strokes drawn zoomed in (where they are tiny in board units). */
export function inkPathData(el: InkEl) {
  return outlineToPath(strokeOutline(el.points, el.size, el.type === 'highlighter'), 100 / Math.min(1, el.size))
}

/** Same curves as outlineToPath, built directly: no string to format and parse, no rounding. */
export function outlinePath(outline: number[][]): Path2D {
  const p = new Path2D()
  const n = outline.length
  if (!n) return p
  p.moveTo(outline[0][0], outline[0][1])
  for (let i = 0; i < n; i++) {
    const [x0, y0] = outline[i]
    const [x1, y1] = outline[(i + 1) % n]
    p.quadraticCurveTo(x0, y0, (x0 + x1) / 2, (y0 + y1) / 2)
  }
  p.closePath()
  return p
}

const lineCache = new WeakMap<El, Path2D>()
function centreLine(el: InkEl) {
  let p = lineCache.get(el)
  if (!p) {
    p = new Path2D()
    const pts = el.points
    p.moveTo(pts[0], pts[1])
    for (let i = 3; i < pts.length; i += 3) p.lineTo(pts[i], pts[i + 1])
    lineCache.set(el, p)
  }
  return p
}

function inkPath(el: InkEl) {
  let p = pathCache.get(el)
  if (!p) {
    p = outlinePath(strokeOutline(el.points, el.size, el.type === 'highlighter'))
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

/**
 * Arrow head length: proportional to the line width only. A minimum in board units would make
 * arrows drawn zoomed in (thin in board units) grow huge heads.
 */
const headLength = (width: number) => width * 3.6

/** Width of the outline drawn around an arrow head, which rounds its corners. */
export const headStroke = (width: number) => width * 0.5

/** Arrow head triangle at (x2, y2) pointing away from (x1, y1). */
export function arrowHead(x1: number, y1: number, x2: number, y2: number, width: number): number[] {
  const len = headLength(width)
  const a = Math.atan2(y2 - y1, x2 - x1)
  const spread = Math.PI / 7
  return [x2, y2, x2 - len * Math.cos(a - spread), y2 - len * Math.sin(a - spread), x2 - len * Math.cos(a + spread), y2 - len * Math.sin(a + spread)]
}

/** Where the line body should end so it doesn't poke through the arrow tip. */
export function lineEnds(el: LineEl) {
  const [x1, y1, x2, y2] = el.points
  const len = Math.hypot(x2 - x1, y2 - y1) || 1
  const back = headLength(el.strokeWidth) * 0.6
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
  /**
   * One device pixel in board units, when drawing on screen. Strokes thinner than that are drawn
   * as their centre line: same look at that size, several times faster to rasterize.
   */
  pixel?: number
  /** Minimap: those strokes stay one pixel wide instead of fading out. */
  hairline?: boolean
  /** On screen: CSS pixels per board unit, for things that keep their size on screen (section titles). */
  zoom?: number
}

function transformTo(ctx: Ctx, el: El) {
  ctx.translate(el.x + el.w / 2, el.y + el.h / 2)
  if (el.rotation) ctx.rotate(el.rotation)
  ctx.translate(-el.w / 2, -el.h / 2)
}

export function drawElement(ctx: Ctx, el: El, env: DrawEnv) {
  if (el.hidden) return
  if (el.erase?.length) drawErased(ctx, el, env)
  else drawBody(ctx, el, env)
}

/* Pixel eraser: the element is painted on a scratch canvas, the marks cut out of it, the result copied over. */

const TILE = 2048
let scratch: OffscreenCanvas | HTMLCanvasElement | null = null
let scratchCtx: Ctx | null = null
function scratchFor(w: number, h: number): Ctx {
  if (!scratch) {
    scratch = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(w, h) : Object.assign(document.createElement('canvas'), { width: w, height: h })
    scratchCtx = scratch.getContext('2d') as Ctx
  }
  if (scratch.width < w) scratch.width = w
  if (scratch.height < h) scratch.height = h
  return scratchCtx!
}

const markCache = new WeakMap<EraseMark, Path2D>()
function markPath(m: EraseMark) {
  let p = markCache.get(m)
  if (!p) {
    p = new Path2D()
    p.moveTo(m.p[0], m.p[1])
    for (let i = 2; i < m.p.length; i += 2) p.lineTo(m.p[i], m.p[i + 1])
    // A tap is a zero-length line, which canvas doesn't stroke: give it a hair of length.
    if (m.p.every((v, i) => v === m.p[i % 2])) p.lineTo(m.p[0] + m.s * 0.001, m.p[1])
    markCache.set(m, p)
  }
  return p
}

function drawErased(ctx: Ctx, el: El, env: DrawEnv) {
  const t = ctx.getTransform()
  const b = aabb(el)
  const xs: number[] = []
  const ys: number[] = []
  for (const [x, y] of [
    [b.x, b.y],
    [b.x + b.w, b.y],
    [b.x, b.y + b.h],
    [b.x + b.w, b.y + b.h],
  ]) {
    xs.push(t.a * x + t.c * y + t.e)
    ys.push(t.b * x + t.d * y + t.f)
  }
  // Only the part on the canvas, in tiles so a huge export never needs a huge scratch canvas.
  const x0 = Math.max(0, Math.floor(Math.min(...xs)) - 1)
  const y0 = Math.max(0, Math.floor(Math.min(...ys)) - 1)
  const x1 = Math.min(ctx.canvas.width, Math.ceil(Math.max(...xs)) + 1)
  const y1 = Math.min(ctx.canvas.height, Math.ceil(Math.max(...ys)) + 1)
  for (let ty = y0; ty < y1; ty += TILE)
    for (let tx = x0; tx < x1; tx += TILE) {
      const w = Math.min(TILE, x1 - tx)
      const h = Math.min(TILE, y1 - ty)
      const s = scratchFor(w, h)
      s.setTransform(1, 0, 0, 1, 0, 0)
      s.globalCompositeOperation = 'source-over'
      s.globalAlpha = 1
      s.clearRect(0, 0, w, h)
      s.setTransform(t.a, t.b, t.c, t.d, t.e - tx, t.f - ty)
      drawBody(s, el, env)
      s.save()
      transformTo(s, el)
      s.globalCompositeOperation = 'destination-out'
      s.lineCap = 'round'
      s.lineJoin = 'round'
      s.strokeStyle = '#000'
      for (const m of el.erase!) {
        s.globalAlpha = Math.max(0, Math.min(1, m.a))
        s.lineWidth = m.s
        s.stroke(markPath(m))
      }
      s.restore()
      ctx.save()
      ctx.setTransform(1, 0, 0, 1, 0, 0)
      ctx.drawImage(scratch!, 0, 0, w, h, tx, ty, w, h)
      ctx.restore()
    }
}

function drawBody(ctx: Ctx, el: El, env: DrawEnv) {
  ctx.save()
  ctx.globalAlpha = Math.max(0, Math.min(1, el.opacity))
  transformTo(ctx, el)

  switch (el.type) {
    case 'ink':
    case 'highlighter':
      if (el.type === 'highlighter') ctx.globalAlpha *= 0.45
      // Thinner than a device pixel (far zoomed out): the canvas draws such lines on a fast path.
      if (env.pixel && el.size <= env.pixel && el.points.length > 3) {
        ctx.strokeStyle = safeColor(el.color)
        ctx.lineWidth = env.hairline ? env.pixel : el.size
        ctx.lineJoin = 'bevel'
        ctx.stroke(centreLine(el))
        break
      }
      ctx.fillStyle = safeColor(el.color)
      ctx.fill(inkPath(el))
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
      if (el.text && env.editingId !== el.id) {
        const t = shapeTextLayout(el)
        ctx.font = fontCss(el.font ?? 'sans', t.size)
        ctx.fillStyle = shapeTextColor(el)
        ctx.textBaseline = 'middle'
        drawLines(ctx, t.layout.lines, 'center', t.width, t.top, t.size, t.left)
      }
      break
    }
    case 'comment': {
      if (env.hairline) break
      const s = COMMENT_PIN / (env.zoom ?? 1)
      const first = el.thread[0]
      ctx.beginPath()
      ctx.roundRect(0, -s, s, s, [s / 2, s / 2, s / 2, 0])
      ctx.fillStyle = '#FFFFFF'
      ctx.shadowColor = 'rgba(0,0,0,0.25)'
      ctx.shadowBlur = 6 / (env.zoom ?? 1)
      ctx.shadowOffsetY = 1 / (env.zoom ?? 1)
      ctx.fill()
      ctx.shadowColor = 'transparent'
      const color = safeColor(first?.color ?? '#0D99FF', '#0D99FF')
      ctx.fillStyle = color
      ctx.beginPath()
      ctx.arc(s / 2, -s / 2, s * 0.36, 0, Math.PI * 2)
      ctx.fill()
      ctx.fillStyle = isDark(color) ? '#FFFFFF' : '#1E1E1E'
      ctx.font = `600 ${s * 0.36}px "Inter Variable", system-ui, sans-serif`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillText(first ? (first.author.trim()[0] ?? '?').toUpperCase() : '+', s / 2, -s / 2 + s * 0.02)
      if (el.thread.length > 1) {
        ctx.font = `600 ${s * 0.32}px "Inter Variable", system-ui, sans-serif`
        ctx.fillStyle = '#1E1E1E'
        ctx.textAlign = 'left'
        ctx.fillText(String(el.thread.length), s * 1.12, -s / 2)
      }
      break
    }
    case 'section': {
      const fill = safeColor(el.fill, '#FFFFFF')
      ctx.fillStyle = fill
      ctx.beginPath()
      ctx.roundRect(0, 0, el.w, el.h, Math.min(env.zoom ? 8 / env.zoom : 8, el.w / 2, el.h / 2))
      ctx.fill()
      ctx.strokeStyle = isDark(fill) ? 'rgba(255,255,255,0.18)' : 'rgba(0,0,0,0.14)'
      ctx.lineWidth = env.zoom ? 1 / env.zoom : 1
      ctx.stroke()
      if (!env.hairline) drawSectionTitle(ctx, el, env.zoom ?? 1, env.editingId === el.id)
      break
    }
    case 'line': {
      if (el.tape) {
        drawTape(ctx, el)
        break
      }
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
        ctx.lineWidth = headStroke(el.strokeWidth)
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
      if (showAuthor(el)) {
        const size = el.h * 0.055
        ctx.font = fontCss('sans', size)
        ctx.fillStyle = isDark(el.color) ? 'rgba(255,255,255,0.75)' : 'rgba(0,0,0,0.6)'
        ctx.textAlign = 'left'
        ctx.textBaseline = 'alphabetic'
        ctx.fillText(el.author!, STICKY_PAD * 0.75, el.h - STICKY_PAD * 0.75, el.w - STICKY_PAD * 1.5)
      }
      if (env.editingId === el.id || !el.text) break
      const { layout, size } = stickyLayout(el)
      ctx.font = fontCss(el.font, size)
      ctx.fillStyle = isDark(el.color) ? '#FFFFFF' : '#1E1E1E'
      ctx.textBaseline = 'middle'
      const top = (el.h - authorBand(el) - layout.height) / 2
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
