import type { Board } from './doc.ts'
import { safeColor } from './doc.ts'
import { aabb, shapePolygon, union } from './geometry.ts'
import { arrowHead, drawElement, inkPathData, isDark, LINE_HEIGHT, lineEnds, STICKY_PAD, stickyLayout, textLayout, type DrawEnv } from './render.ts'
import { preloadStamps, stampDataUrl } from './stamps.ts'
import { FONT_STACK, type Box, type El, type ImageEl, type TextEl } from './types.ts'

export interface ExportOptions {
  /** Export only these elements; null/undefined = the whole board (hidden elements are always skipped). */
  ids?: string[] | null
  /** PNG/PDF pixel ratio, default 2. */
  scale?: number
  /** Fill with the board background (no pattern), default true. */
  background?: boolean
  /** World units around the content, default 32. */
  padding?: number
  /** Image bytes for ImageEl.fileId. */
  fetchFile: (fileId: string) => Promise<Blob>
  /** Raster format, default PNG. JPEG always has the background (it has no transparency). */
  type?: 'image/png' | 'image/jpeg'
  /** PDF only: 'fit' = one page the size of the content, 'a4' = A4 page for printing. */
  page?: 'fit' | 'a4'
}

const MAX_SIDE = 16384
const MAX_AREA = 120_000_000
const EMPTY = "La lavagna è vuota: non c'è niente da esportare."

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D

function collect(board: Board, o: Pick<ExportOptions, 'ids' | 'padding'>): { els: El[]; box: Box } {
  const only = o.ids ? new Set(o.ids) : null
  const els = board.all().filter((e) => !e.hidden && (!only || only.has(e.id)))
  const b = union(els.map(aabb))
  if (!b) throw new Error(EMPTY)
  const p = o.padding ?? 32
  return { els, box: { x: b.x - p, y: b.y - p, w: Math.max(1, b.w + p * 2), h: Math.max(1, b.h + p * 2) } }
}

async function fontsReady() {
  try {
    await document.fonts?.ready
  } catch {
    /* fonts API missing: draw with whatever is loaded */
  }
}

async function loadBitmaps(els: El[], fetchFile: ExportOptions['fetchFile']) {
  const map = new Map<string, ImageBitmap>()
  const ids = [...new Set(els.filter((e): e is ImageEl => e.type === 'image').map((e) => e.fileId))]
  await Promise.all(
    ids.map(async (id) => {
      try {
        map.set(id, await createImageBitmap(await fetchFile(id)))
      } catch {
        /* the placeholder is drawn instead */
      }
    }),
  )
  return map
}

function makeCanvas(w: number, h: number) {
  if (typeof OffscreenCanvas !== 'undefined') {
    const canvas = new OffscreenCanvas(w, h)
    return { ctx: canvas.getContext('2d')! as Ctx, toBlob: (type: string, q?: number) => canvas.convertToBlob({ type, quality: q }) }
  }
  const canvas = document.createElement('canvas')
  canvas.width = w
  canvas.height = h
  return {
    ctx: canvas.getContext('2d')! as Ctx,
    toBlob: (type: string, q?: number) =>
      new Promise<Blob>((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej(new Error('Impossibile creare l\'immagine.'))), type, q)),
  }
}

/** Draws `els` so that world point (box.x, box.y) lands on (ox, oy) at scale k. */
function paint(ctx: Ctx, els: El[], env: DrawEnv, bg: string | null, outW: number, outH: number, k: number, tx: number, ty: number) {
  if (bg) {
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, outW, outH)
  }
  ctx.save()
  ctx.translate(tx, ty)
  ctx.scale(k, k)
  for (const el of els) drawElement(ctx, el, env)
  ctx.restore()
}

export async function exportPNG(board: Board, o: ExportOptions): Promise<Blob> {
  const { els, box } = collect(board, o)
  const k = Math.min(o.scale ?? 2, MAX_SIDE / Math.max(box.w, box.h), Math.sqrt(MAX_AREA / (box.w * box.h)))
  const w = Math.max(1, Math.round(box.w * k))
  const h = Math.max(1, Math.round(box.h * k))
  await fontsReady()
  if (els.some((e) => e.type === 'stamp')) await preloadStamps()
  const bitmaps = await loadBitmaps(els, o.fetchFile)
  const { ctx, toBlob } = makeCanvas(w, h)
  const jpeg = o.type === 'image/jpeg'
  paint(ctx, els, { images: null, bitmaps }, o.background === false && !jpeg ? null : board.getMeta().background, w, h, w / box.w, (-box.x * w) / box.w, (-box.y * h) / box.h)
  bitmaps.forEach((b) => b.close())
  return jpeg ? toBlob('image/jpeg', 0.92) : toBlob('image/png')
}

export async function thumbnail(board: Board, fetchFile: ExportOptions['fetchFile'], ids?: string[] | null): Promise<Blob | null> {
  const W = 480
  const H = 300
  let c
  try {
    c = collect(board, { padding: 0, ids })
  } catch {
    return null
  }
  const k = Math.min((W - 48) / Math.max(1, c.box.w), (H - 48) / Math.max(1, c.box.h), 1.5)
  await fontsReady()
  if (c.els.some((e) => e.type === 'stamp')) await preloadStamps()
  const bitmaps = await loadBitmaps(c.els, fetchFile)
  const { ctx, toBlob } = makeCanvas(W, H)
  paint(ctx, c.els, { images: null, bitmaps }, board.getMeta().background, W, H, k, W / 2 - (c.box.x + c.box.w / 2) * k, H / 2 - (c.box.y + c.box.h / 2) * k)
  bitmaps.forEach((b) => b.close())
  // Browsers without WebP encoding return a PNG on their own.
  return toBlob('image/webp', 0.8)
}

export async function exportPDF(board: Board, o: ExportOptions): Promise<Blob> {
  const { box } = collect(board, o)
  const png = await exportPNG(board, o)
  const { jsPDF } = await import('jspdf')
  const wpt = box.w * 0.75
  const hpt = box.h * 0.75
  const orientation = wpt >= hpt ? 'landscape' : 'portrait'
  if (o.page === 'a4') {
    // Content scaled to fit an A4 sheet with 1.5 cm margins, centred.
    const pdf = new jsPDF({ unit: 'pt', format: 'a4', orientation, compress: true })
    const pw = pdf.internal.pageSize.getWidth()
    const ph = pdf.internal.pageSize.getHeight()
    const k = Math.min((pw - 85) / wpt, (ph - 85) / hpt)
    pdf.addImage(new Uint8Array(await png.arrayBuffer()), 'PNG', (pw - wpt * k) / 2, (ph - hpt * k) / 2, wpt * k, hpt * k)
    return pdf.output('blob')
  }
  const pdf = new jsPDF({ unit: 'pt', format: [wpt, hpt], orientation, compress: true })
  pdf.addImage(new Uint8Array(await png.arrayBuffer()), 'PNG', 0, 0, wpt, hpt)
  return pdf.output('blob')
}

/* ---------------- SVG ---------------- */

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;')
const f = (v: number) => (Number.isFinite(v) ? String(Math.round(v * 100) / 100) : '0')
const col = (c: string, fallback?: string) => esc(safeColor(c, fallback))

function toDataUrl(blob: Blob) {
  return new Promise<string>((res, rej) => {
    const r = new FileReader()
    r.onload = () => res(r.result as string)
    r.onerror = () => rej(r.error)
    r.readAsDataURL(blob)
  })
}

const weight = (el: Pick<TextEl, 'bold' | 'font'>) => (el.bold ? 650 : el.font === 'hand' ? 500 : 400)

function tspans(lines: string[], align: string, width: number, top: number, size: number, left = 0) {
  const x = left + (align === 'center' ? width / 2 : align === 'right' ? width : 0)
  return lines.map((l, i) => `<tspan x="${f(x)}" y="${f(top + size * LINE_HEIGHT * (i + 0.5))}">${esc(l)}</tspan>`).join('')
}

const anchor = (align: string) => (align === 'center' ? 'middle' : align === 'right' ? 'end' : 'start')

function svgBody(el: El, image: string | null): string {
  const dash = (sw: number) => ` stroke-dasharray="${f(sw * 3)} ${f(sw * 2.2)}"`
  switch (el.type) {
    case 'ink':
    case 'highlighter':
      return `<path d="${inkPathData(el)}" fill="${col(el.color)}"${el.type === 'highlighter' ? ' fill-opacity="0.45"' : ''}/>`
    case 'shape': {
      const fill = el.fill !== 'transparent' ? col(el.fill) : 'none'
      const stroke = el.strokeWidth > 0 && el.stroke !== 'transparent' ? ` stroke="${col(el.stroke)}" stroke-width="${f(el.strokeWidth)}" stroke-linejoin="round"${el.dash ? dash(el.strokeWidth) : ''}` : ''
      const attrs = `fill="${fill}"${stroke}`
      if (el.shape === 'rect') {
        const r = Math.min(el.radius, el.w / 2, el.h / 2)
        return `<rect width="${f(el.w)}" height="${f(el.h)}" rx="${f(r)}" ${attrs}/>`
      }
      if (el.shape === 'ellipse') return `<ellipse cx="${f(el.w / 2)}" cy="${f(el.h / 2)}" rx="${f(el.w / 2)}" ry="${f(el.h / 2)}" ${attrs}/>`
      const poly = shapePolygon(el.shape, el.w, el.h, el.points) ?? []
      return `<polygon points="${poly.map(f).join(' ')}" ${attrs}/>`
    }
    case 'line': {
      const { sx, sy, ex, ey } = lineEnds(el)
      const c = col(el.stroke)
      const [x1, y1, x2, y2] = el.points
      let out = `<line x1="${f(sx)}" y1="${f(sy)}" x2="${f(ex)}" y2="${f(ey)}" stroke="${c}" stroke-width="${f(el.strokeWidth)}" stroke-linecap="round"${el.dash ? dash(el.strokeWidth) : ''}/>`
      for (const head of [el.arrowEnd && arrowHead(x1, y1, x2, y2, el.strokeWidth), el.arrowStart && arrowHead(x2, y2, x1, y1, el.strokeWidth)]) {
        if (head) out += `<polygon points="${head.map(f).join(' ')}" fill="${c}" stroke="${c}" stroke-width="${f(Math.max(1, el.strokeWidth * 0.5))}" stroke-linejoin="round"/>`
      }
      return out
    }
    case 'text': {
      const { lines } = textLayout(el)
      return `<text xml:space="preserve" font-family="${esc(FONT_STACK[el.font] ?? FONT_STACK.sans)}" font-size="${f(el.fontSize)}" font-weight="${weight(el)}"${el.italic ? ' font-style="italic"' : ''} fill="${col(el.color)}" text-anchor="${anchor(el.align)}" dominant-baseline="central">${tspans(lines, el.align, el.w, 0, el.fontSize)}</text>`
    }
    case 'sticky': {
      let out = `<rect width="${f(el.w)}" height="${f(el.h)}" rx="4" fill="${col(el.color, '#FFF3A3')}" filter="url(#tratto-shadow)"/>`
      if (el.text) {
        const { layout, size } = stickyLayout(el)
        out += `<text xml:space="preserve" font-family="${esc(FONT_STACK[el.font] ?? FONT_STACK.sans)}" font-size="${f(size)}" font-weight="${el.font === 'hand' ? 500 : 400}" fill="${isDark(el.color) ? '#FFFFFF' : '#1E1E1E'}" text-anchor="${anchor(el.align)}" dominant-baseline="central">${tspans(layout.lines, el.align, el.w - STICKY_PAD * 2, (el.h - layout.height) / 2, size, STICKY_PAD)}</text>`
      }
      return out
    }
    case 'image':
      return image ? `<image width="${f(el.w)}" height="${f(el.h)}" preserveAspectRatio="none" href="${esc(image)}"/>` : ''
    case 'stamp':
      if (image) return `<image width="${f(el.w)}" height="${f(el.h)}" href="${esc(image)}"/>`
      return `<text font-family="'Segoe UI Emoji','Apple Color Emoji','Noto Color Emoji',sans-serif" font-size="${f(el.h * 0.86)}" text-anchor="middle" dominant-baseline="central" x="${f(el.w / 2)}" y="${f(el.h / 2 + el.h * 0.04)}">${esc(el.emoji)}</text>`
  }
}

export async function exportSVG(board: Board, o: ExportOptions): Promise<string> {
  const { els, box } = collect(board, o)
  await fontsReady()
  const images = new Map<string, string | null>()
  await Promise.all(
    [...new Set(els.filter((e): e is ImageEl => e.type === 'image').map((e) => e.fileId))].map(async (id) => {
      try {
        const blob = await o.fetchFile(id)
        images.set(id, blob.type.startsWith('image/') ? await toDataUrl(blob) : null)
      } catch {
        images.set(id, null)
      }
    }),
  )
  const stampUrls = new Map<string, string | null>()
  for (const e of els) if (e.type === 'stamp' && !stampUrls.has(e.emoji)) stampUrls.set(e.emoji, await stampDataUrl(e.emoji).catch(() => null))
  const parts: string[] = []
  for (const el of els) {
    const body = svgBody(el, el.type === 'image' ? (images.get(el.fileId) ?? null) : el.type === 'stamp' ? (stampUrls.get(el.emoji) ?? null) : null)
    if (!body) continue
    const rot = el.rotation ? ` rotate(${f((el.rotation * 180) / Math.PI)})` : ''
    const op = el.opacity < 1 ? ` opacity="${f(Math.max(0, el.opacity))}"` : ''
    parts.push(`<g transform="translate(${f(el.x + el.w / 2)} ${f(el.y + el.h / 2)})${rot} translate(${f(-el.w / 2)} ${f(-el.h / 2)})"${op}>${body}</g>`)
  }
  const bg = o.background === false ? '' : `<rect x="${f(box.x)}" y="${f(box.y)}" width="${f(box.w)}" height="${f(box.h)}" fill="${esc(board.getMeta().background)}"/>`
  const defs = els.some((e) => e.type === 'sticky')
    ? '<defs><filter id="tratto-shadow" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="3" stdDeviation="5" flood-color="#000" flood-opacity="0.14"/></filter></defs>'
    : ''
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${f(Math.ceil(box.w))}" height="${f(Math.ceil(box.h))}" viewBox="${f(box.x)} ${f(box.y)} ${f(box.w)} ${f(box.h)}">${defs}${bg}${parts.join('')}</svg>`
}

/* ---------------- files ---------------- */

export function download(data: Blob | string, filename: string, mime = 'text/plain') {
  const blob = typeof data === 'string' ? new Blob([data], { type: mime }) : data
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.style.display = 'none'
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function safeFilename(title: string) {
  // eslint-disable-next-line no-control-regex
  const s = title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '').trim().slice(0, 80).replace(/[. ]+$/, '')
  return s || 'Lavagna'
}
