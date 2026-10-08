import { getStroke } from 'perfect-freehand'
import { distToSegment, type Pt } from './geometry.ts'

/**
 * Outline polygon of a stroke. `pts` is flat [x, y, pressure, …]; pressure < 0 means the
 * device had no pressure (mouse, finger) and the line keeps an even width.
 *
 * Jitter is already removed while drawing (see OneEuro2D), so streamline stays low: a high
 * value cuts the corners of small loops and makes handwriting look squashed.
 */
export function strokeOutline(pts: number[], size: number, highlighter: boolean): number[][] {
  const input: number[][] = []
  let real = false
  for (let i = 0; i < pts.length; i += 3) {
    input.push([pts[i], pts[i + 1], pts[i + 2] < 0 ? 0.5 : pts[i + 2]])
    if (pts[i + 2] >= 0) real = true
  }
  return getStroke(input, {
    size,
    // Never derive width from speed: slow movements would swell into round blobs.
    simulatePressure: false,
    thinning: highlighter || !real ? 0 : 0.42,
    smoothing: 0.5,
    streamline: highlighter ? 0.3 : 0.18,
    easing: (t) => t,
    // Same shape while drawing and after release, so nothing jumps when the pen lifts.
    last: true,
    start: { cap: true, taper: 0 },
    end: { cap: true, taper: 0 },
  })
}

/**
 * The "1€ filter" (Casiez et al.): heavy smoothing when the pen moves slowly, where tremor is
 * visible, and almost none when it moves fast, where lag would be. Works in screen pixels.
 */
export class OneEuro2D {
  private x = 0
  private y = 0
  private dx = 0
  private dy = 0
  private t = -1
  private readonly minCutoff: number
  private readonly beta: number

  constructor(minCutoff: number, beta = 0.012) {
    this.minCutoff = minCutoff
    this.beta = beta
  }

  private static alpha(cutoff: number, dt: number) {
    const tau = 1 / (2 * Math.PI * cutoff)
    return 1 / (1 + tau / dt)
  }

  filter(x: number, y: number, tMs: number): Pt {
    if (this.t < 0) {
      this.x = x
      this.y = y
      this.t = tMs
      return { x, y }
    }
    const dt = Math.max((tMs - this.t) / 1000, 1 / 1000)
    this.t = tMs
    const ad = OneEuro2D.alpha(1, dt)
    this.dx += ad * ((x - this.x) / dt - this.dx)
    this.dy += ad * ((y - this.y) / dt - this.dy)
    const a = OneEuro2D.alpha(this.minCutoff + this.beta * Math.hypot(this.dx, this.dy), dt)
    this.x += a * (x - this.x)
    this.y += a * (y - this.y)
    return { x: this.x, y: this.y }
  }
}

/** Minimum filter cut-off (Hz) for each "Levigatura del tratto" setting. */
export const SMOOTHING_CUTOFF = { low: 7, medium: 3.5, high: 1.8 } as const

/**
 * Pen pressure as stored in a stroke: smoothed (pens report noisy values, which made the line
 * swell and shrink) and lifted so a light touch still leaves a visible line.
 */
export function nextPressure(prev: number | null, raw: number) {
  const p = Math.min(1, Math.max(0, raw))
  const smoothed = prev === null ? Math.max(p, 0.35) : prev + (p - prev) * 0.3
  return smoothed
}

export const storedPressure = (smoothed: number) => Math.round((0.2 + 0.8 * smoothed) * 1000) / 1000

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
 * stroke doesn't look like one, so the ink is kept as drawn. Strokes smaller than `minSize`
 * (world units; callers pass ~48 screen pixels) are handwriting, never shapes.
 */
export function recognize(flat: number[], minSize = 0): Recognized | null {
  const pts: Pt[] = []
  for (let i = 0; i < flat.length; i += 3) pts.push({ x: flat[i], y: flat[i + 1] })
  if (pts.length < 4) return null
  let length = 0
  for (let i = 1; i < pts.length; i++) length += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y)
  if (length < Math.max(24, minSize * 2)) return null
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
  if (Math.max(box.w, box.h) < Math.max(12, minSize) || Math.min(box.w, box.h) < 12 || !hullArea) return null
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
