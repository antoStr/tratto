import type { Box, Camera, El, ShapeKind } from './types.ts'

export type Pt = { x: number; y: number }

export const toWorld = (cam: Camera, sx: number, sy: number): Pt => ({ x: (sx - cam.x) / cam.z, y: (sy - cam.y) / cam.z })
export const toScreen = (cam: Camera, wx: number, wy: number): Pt => ({ x: wx * cam.z + cam.x, y: wy * cam.z + cam.y })

export function rotate(px: number, py: number, cx: number, cy: number, angle: number): Pt {
  if (!angle) return { x: px, y: py }
  const c = Math.cos(angle)
  const s = Math.sin(angle)
  const dx = px - cx
  const dy = py - cy
  return { x: cx + dx * c - dy * s, y: cy + dx * s + dy * c }
}

export const center = (b: Box): Pt => ({ x: b.x + b.w / 2, y: b.y + b.h / 2 })

/** How far the visible stroke reaches past the element's box. */
export function strokePad(el: El) {
  if (el.type === 'ink' || el.type === 'highlighter') return el.size / 2 + 1
  if (el.type === 'line') return el.strokeWidth * (el.arrowEnd || el.arrowStart ? 3 : 0.5) + 1
  if (el.type === 'shape') return el.strokeWidth / 2 + 1
  return 0
}

export function corners(el: El | Box & { rotation?: number }, pad = 0): Pt[] {
  const c = center(el)
  const r = el.rotation ?? 0
  return [
    [el.x - pad, el.y - pad],
    [el.x + el.w + pad, el.y - pad],
    [el.x + el.w + pad, el.y + el.h + pad],
    [el.x - pad, el.y + el.h + pad],
  ].map(([x, y]) => rotate(x, y, c.x, c.y, r))
}

export function boundsOf(points: Pt[]): Box {
  let minX = Infinity
  let minY = Infinity
  let maxX = -Infinity
  let maxY = -Infinity
  for (const p of points) {
    if (p.x < minX) minX = p.x
    if (p.y < minY) minY = p.y
    if (p.x > maxX) maxX = p.x
    if (p.y > maxY) maxY = p.y
  }
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY }
}

/** Axis-aligned box around everything the element paints. */
export const aabb = (el: El): Box => boundsOf(corners(el, strokePad(el)))

/** Axis-aligned box around the element's own frame (no stroke padding): what selection handles use. */
export const frameBox = (el: El): Box => boundsOf(corners(el))

export function union(boxes: Box[]): Box | null {
  if (!boxes.length) return null
  return boundsOf(boxes.flatMap((b) => [{ x: b.x, y: b.y }, { x: b.x + b.w, y: b.y + b.h }]))
}

export const intersects = (a: Box, b: Box) => a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h

export const expand = (b: Box, d: number): Box => ({ x: b.x - d, y: b.y - d, w: b.w + d * 2, h: b.h + d * 2 })

export function normalizeBox(x1: number, y1: number, x2: number, y2: number): Box {
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) }
}

export function distToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const dx = bx - ax
  const dy = by - ay
  const len = dx * dx + dy * dy
  let t = len ? ((px - ax) * dx + (py - ay) * dy) / len : 0
  t = Math.max(0, Math.min(1, t))
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy))
}

/** Shortest distance between segments AB and CD. */
export function segmentDistance(ax: number, ay: number, bx: number, by: number, cx: number, cy: number, dx: number, dy: number) {
  const cross = (ox: number, oy: number, px: number, py: number, qx: number, qy: number) => (px - ox) * (qy - oy) - (py - oy) * (qx - ox)
  const d1 = cross(ax, ay, bx, by, cx, cy)
  const d2 = cross(ax, ay, bx, by, dx, dy)
  const d3 = cross(cx, cy, dx, dy, ax, ay)
  const d4 = cross(cx, cy, dx, dy, bx, by)
  if (((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))) return 0
  return Math.min(
    distToSegment(ax, ay, cx, cy, dx, dy),
    distToSegment(bx, by, cx, cy, dx, dy),
    distToSegment(cx, cy, ax, ay, bx, by),
    distToSegment(dx, dy, ax, ay, bx, by),
  )
}

/** Even-odd test; `poly` is flat [x, y, …]. */
export function pointInPolygon(x: number, y: number, poly: number[]) {
  let inside = false
  for (let i = 0, j = poly.length - 2; i < poly.length; j = i, i += 2) {
    const xi = poly[i]
    const yi = poly[i + 1]
    const xj = poly[j]
    const yj = poly[j + 1]
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside
  }
  return inside
}

/** Outline of a shape as flat [x, y, …] in local coordinates; null for rect/ellipse, which are drawn natively. */
export function shapePolygon(kind: ShapeKind, w: number, h: number, custom?: number[]): number[] | null {
  switch (kind) {
    case 'triangle':
      return [w / 2, 0, w, h, 0, h]
    case 'diamond':
      return [w / 2, 0, w, h / 2, w / 2, h, 0, h / 2]
    case 'hexagon':
      return [w * 0.25, 0, w * 0.75, 0, w, h / 2, w * 0.75, h, w * 0.25, h, 0, h / 2]
    case 'star': {
      const out: number[] = []
      for (let i = 0; i < 10; i++) {
        const r = i % 2 ? 0.4 : 1
        const a = -Math.PI / 2 + (i * Math.PI) / 5
        out.push(w / 2 + (Math.cos(a) * r * w) / 2, h / 2 + (Math.sin(a) * r * h) / 2 + h * 0.05)
      }
      return out
    }
    case 'polygon':
      return (custom ?? []).map((v, i) => v * (i % 2 ? h : w))
    default:
      return null
  }
}

/** Point in the element's unrotated local space, with (0, 0) at its top-left. */
export function toLocal(el: El, x: number, y: number): Pt {
  const c = center(el)
  const p = rotate(x, y, c.x, c.y, -el.rotation)
  return { x: p.x - el.x, y: p.y - el.y }
}

function nearPolyline(pts: number[], stride: number, x: number, y: number, tol: number, closed: boolean) {
  const n = pts.length
  if (n === stride) return Math.hypot(pts[0] - x, pts[1] - y) <= tol
  for (let i = 0; i + stride < n; i += stride) {
    if (distToSegment(x, y, pts[i], pts[i + 1], pts[i + stride], pts[i + stride + 1]) <= tol) return true
  }
  return closed && n > stride * 2 && distToSegment(x, y, pts[n - stride], pts[n - stride + 1], pts[0], pts[1]) <= tol
}

/** Whether a world point touches the element; `tol` is in world units. */
export function hitTest(el: El, x: number, y: number, tol: number): boolean {
  if (el.hidden) return false
  const p = toLocal(el, x, y)
  switch (el.type) {
    case 'ink':
    case 'highlighter':
      if (p.x < -el.size - tol || p.y < -el.size - tol || p.x > el.w + el.size + tol || p.y > el.h + el.size + tol) return false
      return nearPolyline(el.points, 3, p.x, p.y, el.size / 2 + tol, false)
    case 'line':
      return nearPolyline(el.points, 2, p.x, p.y, el.strokeWidth / 2 + tol, false)
    case 'shape': {
      const filled = el.fill !== 'transparent'
      const edge = el.strokeWidth / 2 + tol
      if (el.shape === 'rect') {
        const inside = p.x >= 0 && p.y >= 0 && p.x <= el.w && p.y <= el.h
        if (filled) return p.x >= -edge && p.y >= -edge && p.x <= el.w + edge && p.y <= el.h + edge
        return (
          (p.x >= -edge && p.y >= -edge && p.x <= el.w + edge && p.y <= el.h + edge) &&
          !(inside && p.x > edge && p.y > edge && p.x < el.w - edge && p.y < el.h - edge)
        )
      }
      if (el.shape === 'ellipse') {
        const rx = el.w / 2
        const ry = el.h / 2
        const nx = (p.x - rx) / Math.max(rx, 0.5)
        const ny = (p.y - ry) / Math.max(ry, 0.5)
        const d = Math.hypot(nx, ny)
        const band = edge / Math.max(Math.min(rx, ry), 1)
        return filled ? d <= 1 + band : Math.abs(d - 1) <= band
      }
      const poly = shapePolygon(el.shape, el.w, el.h, el.points)!
      return (filled && pointInPolygon(p.x, p.y, poly)) || nearPolyline(poly, 2, p.x, p.y, edge, true)
    }
    default:
      return p.x >= -tol && p.y >= -tol && p.x <= el.w + tol && p.y <= el.h + tol
  }
}

/** World-space sample points used by lasso selection. */
export function samplePoints(el: El): Pt[] {
  const c = center(el)
  const toWorldPt = (lx: number, ly: number) => rotate(el.x + lx, el.y + ly, c.x, c.y, el.rotation)
  if (el.type === 'ink' || el.type === 'highlighter') {
    const out: Pt[] = []
    const step = Math.max(3, Math.floor(el.points.length / 3 / 24)) * 3
    for (let i = 0; i < el.points.length; i += step) out.push(toWorldPt(el.points[i], el.points[i + 1]))
    return out
  }
  if (el.type === 'line') return [toWorldPt(el.points[0], el.points[1]), toWorldPt(el.points[2], el.points[3]), c]
  return [...corners(el), c]
}

/**
 * New geometry for an element when its selection box goes from `from` to `to`.
 * Rotated elements keep their rotation; their centre and size follow the scale.
 */
export function scaleElement(el: El, from: Box, to: Box): Partial<El> {
  const sx = from.w > 0.01 ? to.w / from.w : 1
  const sy = from.h > 0.01 ? to.h / from.h : 1
  const c = center(el)
  const ncx = to.x + (c.x - from.x) * sx
  const ncy = to.y + (c.y - from.y) * sy
  // For a rotated element, project the box scale onto its own axes.
  const cos = Math.abs(Math.cos(el.rotation))
  const sin = Math.abs(Math.sin(el.rotation))
  const ex = sx * cos + sy * sin
  const ey = sx * sin + sy * cos
  const w = Math.max(el.w * ex, 0)
  const h = Math.max(el.h * ey, 0)
  const patch: Record<string, unknown> = { x: ncx - w / 2, y: ncy - h / 2, w, h }
  if (el.type === 'ink' || el.type === 'highlighter') {
    const kx = el.w > 0.01 ? w / el.w : ex
    const ky = el.h > 0.01 ? h / el.h : ey
    patch.points = el.points.map((v, i) => (i % 3 === 0 ? v * kx : i % 3 === 1 ? v * ky : v))
    patch.size = Math.max(0.5, el.size * Math.sqrt(Math.abs(ex * ey)))
  } else if (el.type === 'line') {
    const kx = el.w > 0.01 ? w / el.w : 1
    const ky = el.h > 0.01 ? h / el.h : 1
    patch.points = el.points.map((v, i) => (i % 2 ? v * ky : v * kx))
  } else if (el.type === 'stamp') {
    const s = Math.max(w, h)
    Object.assign(patch, { w: s, h: s, x: ncx - s / 2, y: ncy - s / 2 })
  }
  return patch as Partial<El>
}

export function snapAngle(angle: number, step = Math.PI / 12) {
  return Math.round(angle / step) * step
}

export const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v))
