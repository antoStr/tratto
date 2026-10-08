import { getStroke } from 'perfect-freehand'
import { distToSegment, type Pt } from './geometry.ts'

/** Outline polygon of a stroke. `pts` is flat [x, y, pressure, …]; pressure < 0 means "no real pressure" (mouse). */
export function strokeOutline(pts: number[], size: number, highlighter: boolean, complete = true): number[][] {
  const input: number[][] = []
  let real = false
  for (let i = 0; i < pts.length; i += 3) {
    input.push([pts[i], pts[i + 1], pts[i + 2] < 0 ? 0.5 : pts[i + 2]])
    if (pts[i + 2] >= 0) real = true
  }
  return getStroke(input, {
    size,
    thinning: highlighter ? 0 : 0.55,
    smoothing: 0.6,
    streamline: highlighter ? 0.6 : 0.45,
    simulatePressure: !real && !highlighter,
    last: complete,
    start: { cap: true },
    end: { cap: true },
  })
}

/** SVG path data for an outline polygon (quadratic curves through midpoints). */
export function outlineToPath(outline: number[][]): string {
  if (!outline.length) return ''
  const r = (n: number) => Math.round(n * 100) / 100
  const d = outline.reduce<(string | number)[]>(
    (acc, [x0, y0], i, arr) => {
      const [x1, y1] = arr[(i + 1) % arr.length]
      acc.push(r(x0), r(y0), r((x0 + x1) / 2), r((y0 + y1) / 2))
      return acc
    },
    ['M', r(outline[0][0]), r(outline[0][1]), 'Q'],
  )
  d.push('Z')
  return d.join(' ')
}

/** Drops points closer than `minDist` to the previous kept one; keeps the last point. */
export function thinPoints(pts: number[], minDist: number): number[] {
  if (pts.length <= 6) return pts
  const out = [pts[0], pts[1], pts[2]]
  for (let i = 3; i < pts.length - 3; i += 3) {
    const lx = out[out.length - 3]
    const ly = out[out.length - 2]
    if (Math.hypot(pts[i] - lx, pts[i + 1] - ly) >= minDist) out.push(pts[i], pts[i + 1], pts[i + 2])
  }
  out.push(pts[pts.length - 3], pts[pts.length - 2], pts[pts.length - 1])
  return out
}

/* ---------------- ink to shape ---------------- */

export type Recognized =
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number }
  | { kind: 'rect' | 'ellipse' | 'diamond'; x: number; y: number; w: number; h: number }
  | { kind: 'triangle'; points: Pt[] }

function hull(points: Pt[]): Pt[] {
  const p = [...points].sort((a, b) => a.x - b.x || a.y - b.y)
  if (p.length < 3) return p
  const cross = (o: Pt, a: Pt, b: Pt) => (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
  const lower: Pt[] = []
  for (const pt of p) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], pt) <= 0) lower.pop()
    lower.push(pt)
  }
  const upper: Pt[] = []
  for (let i = p.length - 1; i >= 0; i--) {
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p[i]) <= 0) upper.pop()
    upper.push(p[i])
  }
  return lower.slice(0, -1).concat(upper.slice(0, -1))
}

const area = (poly: Pt[]) => Math.abs(poly.reduce((s, p, i) => {
  const q = poly[(i + 1) % poly.length]
  return s + p.x * q.y - q.x * p.y
}, 0)) / 2

/**
 * Guesses which simple shape a freehand stroke was meant to be. Returns null when the
 * stroke doesn't look like one, so the ink is kept as drawn.
 */
export function recognize(flat: number[]): Recognized | null {
  const pts: Pt[] = []
  for (let i = 0; i < flat.length; i += 3) pts.push({ x: flat[i], y: flat[i + 1] })
  if (pts.length < 4) return null
  let length = 0
  for (let i = 1; i < pts.length; i++) length += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y)
  if (length < 24) return null
  const first = pts[0]
  const last = pts[pts.length - 1]
  const gap = Math.hypot(last.x - first.x, last.y - first.y)

  // Straight line: endpoints far apart and no point strays from the chord.
  if (gap > length * 0.8) {
    const dev = Math.max(...pts.map((p) => distToSegment(p.x, p.y, first.x, first.y, last.x, last.y)))
    if (dev < Math.max(6, length * 0.06)) return { kind: 'line', x1: first.x, y1: first.y, x2: last.x, y2: last.y }
    return null
  }
  if (gap > length * 0.25) return null // open curve, not a closed shape

  const h = hull(pts)
  const hullArea = area(h)
  const xs = pts.map((p) => p.x)
  const ys = pts.map((p) => p.y)
  const box = { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) }
  if (box.w < 12 || box.h < 12 || !hullArea) return null
  const fill = hullArea / (box.w * box.h)

  // Largest triangle inside the hull: close to the whole hull means it is a triangle.
  let best: Pt[] = []
  let bestArea = 0
  for (let i = 0; i < h.length; i++)
    for (let j = i + 1; j < h.length; j++)
      for (let k = j + 1; k < h.length; k++) {
        const a = area([h[i], h[j], h[k]])
        if (a > bestArea) {
          bestArea = a
          best = [h[i], h[j], h[k]]
        }
      }
  if (bestArea / hullArea > 0.86) return { kind: 'triangle', points: best }

  // Ellipse: hull points sit close to the ellipse inscribed in the box.
  const rx = box.w / 2
  const ry = box.h / 2
  const cx = box.x + rx
  const cy = box.y + ry
  const ellipseErr = pts.reduce((s, p) => s + Math.abs(Math.hypot((p.x - cx) / rx, (p.y - cy) / ry) - 1), 0) / pts.length
  if (ellipseErr < 0.11 && fill < 0.9) return { kind: 'ellipse', ...box }
  if (fill > 0.84) return { kind: 'rect', ...box }
  if (fill > 0.4 && fill < 0.62) return { kind: 'diamond', ...box }
  return null
}

/* ---------------- ruler ---------------- */

export interface Ruler {
  visible: boolean
  /** Centre in screen pixels. */
  x: number
  y: number
  angle: number
}

export const RULER_LENGTH = 760
export const RULER_HEIGHT = 84

/**
 * If a screen point starts within reach of one of the ruler's long edges, returns a
 * function projecting later points onto that edge (offset by the pen radius).
 */
export function rulerSnapper(r: Ruler, sx: number, sy: number, radius: number): ((x: number, y: number) => Pt) | null {
  if (!r.visible) return null
  const ux = Math.cos(r.angle)
  const uy = Math.sin(r.angle)
  const nx = -uy
  const ny = ux
  const dx = sx - r.x
  const dy = sy - r.y
  const along = dx * ux + dy * uy
  const across = dx * nx + dy * ny
  if (Math.abs(along) > RULER_LENGTH / 2 + 30) return null
  const half = RULER_HEIGHT / 2
  for (const side of [-1, 1]) {
    const edge = side * half
    // Start within 36px outside the edge (or a little inside it).
    if ((across - edge) * side > -10 && (across - edge) * side < 36) {
      const offset = edge + side * radius
      return (x, y) => {
        const t = (x - r.x) * ux + (y - r.y) * uy
        return { x: r.x + ux * t + nx * offset, y: r.y + uy * t + ny * offset }
      }
    }
  }
  return null
}

/** Whether a screen point is on the ruler body (used to drag it). */
export function onRuler(r: Ruler, sx: number, sy: number) {
  if (!r.visible) return false
  const dx = sx - r.x
  const dy = sy - r.y
  const along = dx * Math.cos(r.angle) + dy * Math.sin(r.angle)
  const across = -dx * Math.sin(r.angle) + dy * Math.cos(r.angle)
  return Math.abs(along) <= RULER_LENGTH / 2 && Math.abs(across) <= RULER_HEIGHT / 2
}
