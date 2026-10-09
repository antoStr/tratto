import type { HocuspocusProvider } from '@hocuspocus/provider'
import { Board, uid } from './doc.ts'
import {
  aabb as box,
  boundsOf,
  center,
  clamp,
  connectable,
  corners,
  distToSegment,
  expand,
  frameBox,
  hitTest,
  intersects,
  lineWorldEnds,
  MIN_SIZE,
  normalizeBox,
  pointInPolygon,
  rotate,
  routeConnector,
  samplePoints,
  scaleElement,
  segmentDistance,
  setLineEnds,
  snapAngle,
  toLocal,
  toScreen,
  toWorld,
  union,
  type Pt,
} from './geometry.ts'
import { nextPressure, OneEuro2D, onRuler, recognize, RULER_LENGTH, rulerSnapper, SMOOTHING_CUTOFF, storedPressure, strokeOutline, thinPoints } from './ink.ts'
import { COMMENT_PIN, drawBackground, drawElement, ImageStore, LINE_HEIGHT, resetTextCaches, layoutText, fontCss, outlinePath, sectionTitleBox } from './render.ts'
import { onStampLoaded } from './stamps.ts'
import { toggleFocus, useEditor, voterId, type Tool } from './store.ts'
import { SECTION_COLORS, type Box, type Camera, type CommentEl, type El, type InkEl, type LineEl, type SectionEl, type ShapeEl, type ShapeKind, type StickyEl, type TextEl } from './types.ts'

export type Awareness = NonNullable<HocuspocusProvider['awareness']>
/** `add-*`: FigJam's "+" beside a shape or sticky: click for a connected copy, drag for a connector. */
type AddHandle = 'add-n' | 'add-e' | 'add-s' | 'add-w'
type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w' | 'rot' | 'p0' | 'p1' | AddHandle
export type AlignKind = 'left' | 'hcenter' | 'right' | 'top' | 'vcenter' | 'bottom'

/** Actions the rest of the UI can trigger on the live canvas. Filled in while a canvas is mounted. */
export const commands = {
  zoomBy: (_f: number) => {},
  zoomTo: (_z: number) => {},
  fit: () => {},
  fitSelection: () => {},
  undo: () => {},
  redo: () => {},
  remove: () => {},
  duplicate: () => {},
  copy: () => {},
  cut: () => {},
  paste: () => {},
  order: (_dir: 'front' | 'back' | 'forward' | 'backward') => {},
  toggleLock: () => {},
  toggleHide: () => {},
  selectAll: () => {},
  align: (_kind: AlignKind) => {},
  distribute: (_axis: 'x' | 'y') => {},
  insert: (_els: El[], _at?: Pt, _opts?: { avoidOverlap?: boolean; focus?: boolean }) => {},
  viewportCenter: (): Pt => ({ x: 0, y: 0 }),
  /** Visible part of the board, in board units. */
  viewport: (): Box => ({ x: 0, y: 0, w: 0, h: 0 }),
  centerOn: (_p: Pt) => {},
  /** Brings elements into view without changing the zoom unless they don't fit. */
  reveal: (_ids: string[]) => {},
  group: () => {},
  ungroup: () => {},
  follow: (_clientId: number | null) => {},
}

export const MIN_ZOOM = 0.04
export const MAX_ZOOM = 32
const CLIP_PREFIX = 'tratto-clipboard:'
const ERASABLE = new Set(['ink', 'highlighter', 'line', 'shape'])
/** What the pixel eraser can cut into (text and stickies stay editable, sections are containers). */
const PIXEL_ERASABLE = new Set(['ink', 'highlighter', 'line', 'shape', 'image', 'stamp'])
/** The pin of a comment on screen, in board units: it keeps its size whatever the zoom. */
const pinBox = (el: El, z: number): Box => ({ x: el.x, y: el.y - COMMENT_PIN / z, w: COMMENT_PIN / z, h: COMMENT_PIN / z })
/** What can receive votes: things with content, not lines, strokes or comments. */
const votable = (el: El) => el.type !== 'line' && el.type !== 'ink' && el.type !== 'highlighter' && el.type !== 'comment'
const inside = (o: Box, i: Box) => i.x >= o.x && i.y >= o.y && i.x + i.w <= o.x + o.w && i.y + i.h <= o.y + o.h
/** A copy of a connector with one end (or both) let go. */
function unbind(l: LineEl, ends: ('from' | 'to')[]): LineEl {
  const next = { ...l }
  for (const k of ends) delete next[k]
  return next
}
const r2 = (n: number) => Math.round(n * 100) / 100
/** Rounds stroke coordinates to 1/100 of the pen width, never coarser than 0.01: strokes drawn zoomed in are tiny. */
const strokeRound = (size: number) => {
  const q = 100 / Math.min(1, size)
  return (n: number) => Math.round(n * q) / q
}

type Gesture =
  | { kind: 'pan'; sx: number; sy: number; cam: Camera }
  | { kind: 'pinch'; ids: [number, number]; d0: number; c0: Pt; cam: Camera; ruler: { x: number; y: number; angle: number; a0: number } | null }
  | {
      kind: 'draw'
      hl: boolean
      color: string
      size: number
      opacity: number
      pts: number[]
      snap: ((x: number, y: number) => Pt) | null
      pointerId: number
      filter: OneEuro2D
      /** Smoothed pen pressure; null until the first sample, never used without real pressure. */
      pressure: number | null
      usePressure: boolean
      /** Last raw position, added at release so the line ends exactly where the pen lifted. */
      raw: Pt | null
    }
  | {
      kind: 'erase'
      last: Pt
      pointerId: number
      /** Pixel mode: count of stretches so far, and per element its state before and the marks of this pass. */
      seg: number
      touched: Map<string, { orig: El; strokes: number[][]; lastSeg: number }>
    }
  /** `tap`: started with the pen's barrel button, which opens the menu when not dragged. */
  | { kind: 'lasso'; poly: number[]; tap?: { clientX: number; clientY: number } }
  | { kind: 'marquee'; start: Pt; cur: Pt; base: string[] }
  | { kind: 'move'; start: Pt; orig: Map<string, El>; box: Box; cands: Box[]; moved: boolean; sx: number; sy: number }
  | { kind: 'resize'; handle: Handle; start: Pt; orig: Map<string, El>; frame: Box; rotation: number }
  | { kind: 'rotate'; c: Pt; a0: number; orig: Map<string, El> }
  | { kind: 'endpoint'; which: 0 | 1; orig: LineEl }
  /** `spawn`: dragged from a "+": released on empty board, a copy of it is made there and connected. */
  | { kind: 'create'; el: El; start: Pt; moved: boolean; spawn?: El }
  | { kind: 'laser' }
  | { kind: 'ruler'; sx: number; sy: number; x: number; y: number }
  | { kind: 'ruler-rotate' }

interface Options {
  root: HTMLDivElement
  scene: HTMLCanvasElement
  overlay: HTMLCanvasElement
  board: Board
  awareness: Awareness | null
  images: ImageStore
  boardKey: string
  onImageFiles: (files: File[], at: Pt) => void
  /** Opens the context menu at a client position (pen barrel button, long press). */
  onMenu: (clientX: number, clientY: number) => void
}

export function createController(o: Options) {
  const { root, scene, overlay, board, awareness, images } = o
  const st = useEditor.getState
  const set = useEditor.setState
  const sctx = scene.getContext('2d')!
  // No `desynchronized`: low-latency canvases tear and flicker on some Windows GPUs.
  const octx = overlay.getContext('2d')!

  let W = 0
  let H = 0
  let dpr = window.devicePixelRatio || 1
  let rect = root.getBoundingClientRect()
  let gesture: Gesture | null = null
  const pointers = new Map<number, { x: number; y: number; type: string }>()
  let lastPen = 0
  let hoverId: string | null = null
  let hoverHandle: Handle | null = null
  let spaceDown = false
  let eraserAt: Pt | null = null
  let guides: { x1: number; y1: number; x2: number; y2: number }[] = []
  const preview = new Map<string, El>()
  let laser: { x: number; y: number; t: number }[] = []
  let following: number | null = null
  /** The "+" pressed, until we know whether it is a click or a drag. */
  let pendingAdd: AddHandle | null = null
  let clipboard: El[] = []
  let pasteCount = 0

  /* ---------- helpers ---------- */

  const cam = () => st().camera
  const readOnly = () => st().readOnly
  const view = (): Box => {
    const c = cam()
    const a = toWorld(c, 0, 0)
    return { x: a.x, y: a.y, w: W / c.z, h: H / c.z }
  }
  const screenPt = (e: { clientX: number; clientY: number }): Pt => ({ x: e.clientX - rect.left, y: e.clientY - rect.top })
  const worldPt = (e: { clientX: number; clientY: number }) => {
    const s = screenPt(e)
    return toWorld(cam(), s.x, s.y)
  }
  const current = (id: string) => preview.get(id) ?? board.get(id)
  const selected = () => st().selection.map(current).filter((e): e is El => !!e)
  const editable = (el: El) => !el.locked && !el.hidden

  function hitElement(p: Pt, tolPx = 6): El | null {
    const all = board.paintOrder()
    const z = cam().z
    const tol = tolPx / z
    for (let i = all.length - 1; i >= 0; i--) {
      const el = current(all[i].id)!
      if (el.locked || el.hidden) continue
      if (el.type === 'section') {
        const t = sectionTitleBox(el, z)
        if (p.x >= t.x && p.y >= t.y && p.x <= t.x + t.w && p.y <= t.y + t.h) return el
      }
      if (el.type === 'comment') {
        const b = expand(pinBox(el, z), tol / 2)
        if (p.x >= b.x && p.y >= b.y && p.x <= b.x + b.w && p.y <= b.y + b.h) return el
        continue
      }
      if (!intersects(expand(box(el), tol), { x: p.x, y: p.y, w: 0, h: 0 })) continue
      if (hitTest(el, p.x, p.y, tol)) return el
    }
    return null
  }

  /* ---------- awareness ---------- */

  let pendingAw: Record<string, unknown> = {}
  let awTimer = 0
  const flushAw = () => {
    awTimer = 0
    if (!awareness) return
    awareness.setLocalState({ ...(awareness.getLocalState() ?? {}), ...pendingAw })
    pendingAw = {}
  }
  const aw = (field: string, value: unknown, now = false) => {
    pendingAw[field] = value
    if (now) {
      clearTimeout(awTimer)
      flushAw()
    } else if (!awTimer) {
      // The host relays every update to everyone: with many people, send less often to spare its upload.
      const people = awareness?.getStates().size ?? 1
      awTimer = window.setTimeout(flushAw, people > 10 ? 50 * (people / 10) : 50)
    }
  }

  /* ---------- camera ---------- */

  const setCam = (c: Camera) => {
    set({ camera: { x: c.x, y: c.y, z: clamp(c.z, MIN_ZOOM, MAX_ZOOM) } })
  }
  const zoomAt = (sx: number, sy: number, z: number) => {
    const c = cam()
    const nz = clamp(z, MIN_ZOOM, MAX_ZOOM)
    const w = toWorld(c, sx, sy)
    setCam({ x: sx - w.x * nz, y: sy - w.y * nz, z: nz })
  }
  const fitBox = (b: Box | null, maxZoom: number) => {
    if (!b) return setCam({ x: W / 2, y: H / 2, z: 1 })
    const pad = 80
    const z = clamp(Math.min((W - pad * 2) / Math.max(b.w, 1), (H - pad * 2) / Math.max(b.h, 1), maxZoom), MIN_ZOOM, MAX_ZOOM)
    setCam({ x: W / 2 - (b.x + b.w / 2) * z, y: H / 2 - (b.y + b.h / 2) * z, z })
  }
  const stopFollowing = () => {
    if (following !== null) {
      following = null
      set({ following: null })
    }
  }

  /* ---------- rendering ---------- */

  let sceneDirty = true
  let raf = 0
  const invalidate = (sceneToo = true) => {
    if (sceneToo) sceneDirty = true
    if (!raf) raf = requestAnimationFrame(frame)
  }

  function frame() {
    raf = 0
    if (sceneDirty) drawScene()
    sceneDirty = false
    drawOverlay()
    if (laser.length || remoteLasers()) invalidate(false)
  }

  function drawScene() {
    const c = cam()
    sctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    drawBackground(sctx, c, W, H, board.getMeta())
    sctx.setTransform(dpr * c.z, 0, 0, dpr * c.z, dpr * c.x, dpr * c.y)
    const v = view()
    const env = { images, editingId: st().editingId, pixel: 1 / (c.z * dpr), zoom: c.z }
    let anyVisible = false
    let anyOnScreen = false
    // A section being drawn goes behind everything, like the others.
    const creating = gesture?.kind === 'create' ? gesture.el : null
    if (creating?.type === 'section') drawElement(sctx, creating, env)
    for (const base of board.paintOrder()) {
      const el = preview.get(base.id) ?? base
      if (!el.hidden) anyVisible = true
      if (intersects(box(el), v)) {
        drawElement(sctx, el, env)
        if (!el.hidden) anyOnScreen = true
      }
    }
    const lost = anyVisible && !anyOnScreen
    if (lost !== st().lost) set({ lost })
    if (creating && creating.type !== 'section') drawElement(sctx, creating, env)
  }

  let ACCENT = st().prefs.accent
  const handleScale = () => (st().prefs.bigHandles ? 1.6 : 1)

  function strokeWorldPath(ctx: CanvasRenderingContext2D, pts: number[], size: number, hl: boolean, color: string, opacity = 1) {
    const c = cam()
    ctx.save()
    ctx.setTransform(dpr * c.z, 0, 0, dpr * c.z, dpr * c.x, dpr * c.y)
    ctx.globalAlpha = (hl ? 0.45 : 1) * clamp(opacity, 0.05, 1)
    ctx.fillStyle = color
    ctx.fill(outlinePath(strokeOutline(pts, size, hl)))
    ctx.restore()
  }

  function outlineEl(ctx: CanvasRenderingContext2D, el: El, color: string, width = 1) {
    const c = cam()
    ctx.strokeStyle = color
    ctx.lineWidth = width
    if (el.type === 'line') {
      const a = toScreen(c, el.x + el.points[0], el.y + el.points[1])
      const b = toScreen(c, el.x + el.points[2], el.y + el.points[3])
      ctx.beginPath()
      ctx.moveTo(a.x, a.y)
      ctx.lineTo(b.x, b.y)
      ctx.stroke()
      return
    }
    const pts = corners(el).map((p) => toScreen(c, p.x, p.y))
    ctx.beginPath()
    pts.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)))
    ctx.closePath()
    ctx.stroke()
  }

  function remoteStates() {
    if (!awareness) return []
    return [...awareness.getStates().entries()].filter(([id]) => id !== awareness.clientID)
  }
  const remoteLasers = () => remoteStates().some(([, s]) => Array.isArray(s.laser) && s.laser.length)

  function drawOverlay() {
    const ctx = octx
    const c = cam()
    ACCENT = /^#[0-9a-f]{6}$/i.test(st().prefs.accent) ? st().prefs.accent : '#0D99FF'
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, overlay.width, overlay.height)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    // Other people: their selection, the stroke they are drawing, their laser.
    for (const [, s] of remoteStates()) {
      const color = typeof s.user?.color === 'string' ? s.user.color : '#999'
      if (Array.isArray(s.sel))
        for (const id of s.sel.slice(0, 200)) {
          const el = typeof id === 'string' && current(id)
          if (el) outlineEl(ctx, el, color, 1.5)
        }
      const live = s.live
      if (live && Array.isArray(live.pts) && live.pts.length >= 3 && typeof live.size === 'number')
        strokeWorldPath(ctx, live.pts, live.size, !!live.hl, /^#[0-9a-f]{6}$/i.test(live.color) ? live.color : color, typeof live.op === 'number' ? live.op : 1)
      if (Array.isArray(s.laser)) drawLaser(ctx, s.laser as number[], color)
    }

    // Hover outline (Figma style).
    const g = gesture
    if (hoverId && (!g || g.kind === 'create' || g.kind === 'endpoint') && !st().selection.includes(hoverId)) {
      const el = current(hoverId)
      if (el) outlineEl(ctx, el, ACCENT, g ? 2 : 1.5)
    }

    // Selection.
    const sel = selected()
    if (sel.length && g?.kind !== 'draw') {
      if (sel.length > 1) for (const el of sel) outlineEl(ctx, el, ACCENT, 1)
      const handles = handlePositions()
      const frame = selectionFrame()
      if (frame && !(sel.length === 1 && sel[0].type === 'line')) {
        const pts = corners({ ...frame.box, rotation: frame.rotation }).map((p) => toScreen(c, p.x, p.y))
        ctx.strokeStyle = ACCENT
        ctx.lineWidth = 1
        ctx.beginPath()
        pts.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)))
        ctx.closePath()
        ctx.stroke()
        if (handles.find((h) => h.h === 'rot')) {
          // The knob hangs from the top edge, or from the top-right corner when the "+" take the top.
          const top = handles.find((h) => h.h === (handles.some((k) => k.h === 'add-n') ? 'ne' : 'n'))
          const rot = handles.find((h) => h.h === 'rot')!
          if (top) {
            ctx.beginPath()
            ctx.moveTo(top.x, top.y)
            ctx.lineTo(rot.x, rot.y)
            ctx.stroke()
          }
        }
      }
      if (sel.length === 1 && sel[0].type === 'line') outlineEl(ctx, sel[0], ACCENT, 1)
      if (!g || g.kind === 'resize' || g.kind === 'rotate' || g.kind === 'endpoint')
        for (const h of handles) {
          const k = handleScale()
          ctx.fillStyle = '#FFFFFF'
          ctx.strokeStyle = ACCENT
          ctx.lineWidth = k > 1 ? 1.5 : 1
          ctx.beginPath()
          if (h.h.startsWith('add-')) {
            if (g) continue
            // FigJam's "+": a filled dot that grows when the pointer is on it.
            const r = (hoverHandle === h.h ? 9 : 7) * k
            ctx.fillStyle = ACCENT
            ctx.arc(h.x, h.y, r, 0, Math.PI * 2)
            ctx.fill()
            ctx.strokeStyle = '#FFFFFF'
            ctx.lineWidth = 1.5
            ctx.beginPath()
            ctx.moveTo(h.x - r * 0.45, h.y)
            ctx.lineTo(h.x + r * 0.45, h.y)
            ctx.moveTo(h.x, h.y - r * 0.45)
            ctx.lineTo(h.x, h.y + r * 0.45)
            ctx.stroke()
            continue
          }
          if (h.h === 'rot' || h.h === 'p0' || h.h === 'p1') ctx.arc(h.x, h.y, 4.5 * k, 0, Math.PI * 2)
          else ctx.rect(Math.round(h.x) - 3.5 * k, Math.round(h.y) - 3.5 * k, 7 * k, 7 * k)
          ctx.fill()
          ctx.stroke()
        }
      // Size label under the selection while transforming.
      if (frame && (g?.kind === 'resize' || g?.kind === 'create')) {
        const b = frame.box
        const bottom = toScreen(c, b.x + b.w / 2, b.y + b.h)
        label(ctx, `${Math.round(b.w)} × ${Math.round(b.h)}`, bottom.x, bottom.y + 18 + (frame.rotation ? 12 : 0))
      }
      if (g?.kind === 'rotate' && sel.length === 1) {
        const b = frameBox(sel[0])
        const bottom = toScreen(c, b.x + b.w / 2, b.y + b.h)
        label(ctx, `${Math.round((((sel[0].rotation * 180) / Math.PI) % 360 + 360) % 360)}°`, bottom.x, bottom.y + 18)
      }
    }

    if (g?.kind === 'create') {
      const b = frameBox(g.el)
      const bottom = toScreen(c, b.x + b.w / 2, b.y + b.h)
      label(ctx, `${Math.round(b.w)} × ${Math.round(b.h)}`, bottom.x, bottom.y + 18)
    }

    // Votes: during the session only your own (as in FigJam), everyone's once it has ended.
    const voting = board.voting()
    if (voting) {
      const counts = voting.ended ? board.tally() : board.tally(voterId())
      for (const [id, n] of counts) {
        const el = current(id)
        if (!el || el.hidden) continue
        const b = frameBox(el)
        const at = toScreen(c, b.x + b.w, b.y)
        const r = 11
        ctx.fillStyle = voting.ended ? '#E8590C' : ACCENT
        ctx.beginPath()
        ctx.arc(at.x - 4, at.y + 4, r, 0, Math.PI * 2)
        ctx.fill()
        ctx.strokeStyle = '#FFFFFF'
        ctx.lineWidth = 2
        ctx.stroke()
        ctx.fillStyle = '#FFFFFF'
        ctx.font = '650 11px "Inter Variable", system-ui, sans-serif'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'middle'
        ctx.fillText(String(n), at.x - 4, at.y + 4.5)
      }
    }

    if (g?.kind === 'marquee') {
      const a = toScreen(c, g.start.x, g.start.y)
      const b = toScreen(c, g.cur.x, g.cur.y)
      const r = normalizeBox(a.x, a.y, b.x, b.y)
      ctx.fillStyle = ACCENT + '14'
      ctx.strokeStyle = ACCENT
      ctx.lineWidth = 1
      ctx.fillRect(r.x, r.y, r.w, r.h)
      ctx.strokeRect(Math.round(r.x) + 0.5, Math.round(r.y) + 0.5, Math.round(r.w), Math.round(r.h))
    }

    if (g?.kind === 'lasso' && g.poly.length > 2) {
      ctx.save()
      ctx.setLineDash([5, 4])
      ctx.strokeStyle = ACCENT
      ctx.fillStyle = ACCENT + '0F'
      ctx.lineWidth = 1.5
      ctx.beginPath()
      for (let i = 0; i < g.poly.length; i += 2) {
        const p = toScreen(c, g.poly[i], g.poly[i + 1])
        if (i) ctx.lineTo(p.x, p.y)
        else ctx.moveTo(p.x, p.y)
      }
      ctx.closePath()
      ctx.fill()
      ctx.stroke()
      ctx.restore()
    }

    if (g?.kind === 'draw' && g.pts.length >= 3) strokeWorldPath(ctx, g.pts, g.size, g.hl, g.color, g.opacity)

    if (guides.length) {
      ctx.strokeStyle = '#F24822'
      ctx.lineWidth = 1
      ctx.beginPath()
      for (const l of guides) {
        const a = toScreen(c, l.x1, l.y1)
        const b = toScreen(c, l.x2, l.y2)
        ctx.moveTo(Math.round(a.x) + 0.5, Math.round(a.y) + 0.5)
        ctx.lineTo(Math.round(b.x) + 0.5, Math.round(b.y) + 0.5)
      }
      ctx.stroke()
    }

    if (laser.length) {
      const now = performance.now()
      laser = laser.filter((p) => now - p.t < 900)
      drawLaser(ctx, laser.flatMap((p) => [p.x, p.y]), '#FF3B30')
    }

    if (eraserAt && (st().tool === 'eraser' || g?.kind === 'erase')) {
      ctx.strokeStyle = 'rgba(0,0,0,0.55)'
      ctx.fillStyle = 'rgba(255,255,255,0.35)'
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.arc(eraserAt.x, eraserAt.y, st().prefs.eraser.size / 2, 0, Math.PI * 2)
      ctx.fill()
      ctx.stroke()
    }
  }

  function drawLaser(ctx: CanvasRenderingContext2D, flat: number[], color: string) {
    const c = cam()
    const n = flat.length / 2
    if (n < 1) return
    ctx.save()
    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'
    ctx.shadowColor = color
    ctx.shadowBlur = 10
    for (let i = 1; i < n; i++) {
      const a = toScreen(c, flat[(i - 1) * 2], flat[(i - 1) * 2 + 1])
      const b = toScreen(c, flat[i * 2], flat[i * 2 + 1])
      ctx.globalAlpha = (i / n) * 0.9
      ctx.strokeStyle = color
      ctx.lineWidth = 2 + (i / n) * 4
      ctx.beginPath()
      ctx.moveTo(a.x, a.y)
      ctx.lineTo(b.x, b.y)
      ctx.stroke()
    }
    const head = toScreen(c, flat[(n - 1) * 2], flat[(n - 1) * 2 + 1])
    ctx.globalAlpha = 1
    ctx.fillStyle = '#FFFFFF'
    ctx.beginPath()
    ctx.arc(head.x, head.y, 3, 0, Math.PI * 2)
    ctx.fill()
    ctx.restore()
  }

  function label(ctx: CanvasRenderingContext2D, text: string, x: number, y: number) {
    ctx.font = '500 11px "Inter Variable", system-ui, sans-serif'
    const w = ctx.measureText(text).width + 10
    ctx.fillStyle = ACCENT
    ctx.beginPath()
    ctx.roundRect(Math.round(x - w / 2), Math.round(y - 9), Math.round(w), 18, 4)
    ctx.fill()
    ctx.fillStyle = '#FFFFFF'
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(text, x, y)
  }

  /* ---------- selection frame and handles ---------- */

  function selectionFrame(): { box: Box; rotation: number } | null {
    const els = selected()
    if (!els.length) return null
    if (els.length === 1 && els[0].type === 'comment') return { box: pinBox(els[0], cam().z), rotation: 0 }
    if (els.length === 1 && els[0].type !== 'line') return { box: { x: els[0].x, y: els[0].y, w: els[0].w, h: els[0].h }, rotation: els[0].rotation }
    return { box: union(els.map(frameBox))!, rotation: 0 }
  }

  function handlePositions(): { h: Handle; x: number; y: number }[] {
    if (readOnly()) return []
    const els = selected()
    const c = cam()
    if (!els.length || els.some((e) => e.locked) || (els.length === 1 && els[0].type === 'comment')) return []
    if (els.length === 1 && els[0].type === 'line') {
      const l = els[0]
      const a = toScreen(c, l.x + l.points[0], l.y + l.points[1])
      const b = toScreen(c, l.x + l.points[2], l.y + l.points[3])
      return [
        { h: 'p0', ...a },
        { h: 'p1', ...b },
      ]
    }
    const f = selectionFrame()!
    const b = f.box
    const cx = b.x + b.w / 2
    const cy = b.y + b.h / 2
    const at = (h: Handle, lx: number, ly: number) => {
      const p = rotate(lx, ly, cx, cy, f.rotation)
      return { h, ...toScreen(c, p.x, p.y) }
    }
    const sw = b.w * c.z
    const sh = b.h * c.z
    const list = [at('nw', b.x, b.y), at('ne', b.x + b.w, b.y), at('se', b.x + b.w, b.y + b.h), at('sw', b.x, b.y + b.h)]
    if (sw > 28) list.push(at('n', cx, b.y), at('s', cx, b.y + b.h))
    if (sh > 28) list.push(at('e', b.x + b.w, cy), at('w', b.x, cy))
    const one = els.length === 1 ? els[0] : null
    const adds = !!one && (one.type === 'shape' || one.type === 'sticky') && sw > 24 && sh > 24
    if (adds) {
      const d = 22 / c.z
      list.push(at('add-n', cx, b.y - d), at('add-e', b.x + b.w + d, cy), at('add-s', cx, b.y + b.h + d), at('add-w', b.x - d, cy))
    }
    if (one?.type === 'section') return list
    if (!(one?.type === 'stamp' && sw < 16)) {
      const d = 22 * handleScale()
      if (adds) {
        // The top "+" sits where the rotation knob would: the knob goes out from the top-right corner.
        const corner = rotate(b.x + b.w, b.y, cx, cy, f.rotation)
        const s = toScreen(c, corner.x, corner.y)
        const a = f.rotation - Math.PI / 4
        list.push({ h: 'rot', x: s.x + Math.sin(a + Math.PI / 2) * d * 0.8, y: s.y - Math.cos(a + Math.PI / 2) * d * 0.8 })
      } else {
        const top = rotate(cx, b.y, cx, cy, f.rotation)
        const s = toScreen(c, top.x, top.y)
        list.push({ h: 'rot', x: s.x + Math.sin(f.rotation) * d, y: s.y - Math.cos(f.rotation) * d })
      }
    }
    return list
  }

  function hitHandle(s: Pt, touch: boolean): Handle | null {
    const r = (touch ? 18 : 9) * handleScale()
    let best: Handle | null = null
    let bestD = r
    for (const h of handlePositions()) {
      const d = Math.hypot(h.x - s.x, h.y - s.y)
      if (d <= bestD) {
        bestD = d
        best = h.h
      }
    }
    return best
  }

  function insideSelection(p: Pt) {
    const f = selectionFrame()
    if (!f) return false
    const cx = f.box.x + f.box.w / 2
    const cy = f.box.y + f.box.h / 2
    const l = rotate(p.x, p.y, cx, cy, -f.rotation)
    return l.x >= f.box.x && l.y >= f.box.y && l.x <= f.box.x + f.box.w && l.y <= f.box.y + f.box.h
  }

  const CURSORS: Record<string, string> = {
    n: 'ns-resize',
    s: 'ns-resize',
    e: 'ew-resize',
    w: 'ew-resize',
    nw: 'nwse-resize',
    se: 'nwse-resize',
    ne: 'nesw-resize',
    sw: 'nesw-resize',
    rot: 'grab',
    p0: 'move',
    p1: 'move',
    'add-n': 'copy',
    'add-e': 'copy',
    'add-s': 'copy',
    'add-w': 'copy',
  }
  function updateCursor() {
    const t = st().tool
    let cur = 'default'
    if (gesture?.kind === 'pan') cur = 'grabbing'
    else if (spaceDown || t === 'hand') cur = 'grab'
    else if (hoverHandle) cur = CURSORS[hoverHandle]
    else if (t === 'pen' || t === 'highlighter' || t === 'shape' || t === 'line' || t === 'arrow' || t === 'lasso' || t === 'laser' || t === 'stamp' || t === 'sticky' || t === 'section' || t === 'tape') cur = 'crosshair'
    else if (t === 'comment') cur = 'cell'
    else if (t === 'eraser') cur = 'none'
    else if (t === 'text') cur = 'text'
    root.style.cursor = cur
  }

  /* ---------- committing edits ---------- */

  let flushTimer = 0
  const flushPreview = () => {
    clearTimeout(flushTimer)
    flushTimer = 0
    if (!preview.size || readOnly()) return
    board.transact(() => {
      for (const [id, el] of preview) if (board.elements.has(id)) board.elements.set(id, el)
    })
  }
  const scheduleFlush = () => {
    if (!flushTimer) flushTimer = window.setTimeout(flushPreview, 110)
  }

  const baseProps = (): Pick<El, 'rotation' | 'z' | 'opacity'> => ({ rotation: 0, z: board.topZ(), opacity: 1 })
  /** Sizes chosen in the toolbar are what you see on screen, whatever the zoom. */
  const worldSize = (screenPx: number) => r2(screenPx / cam().z)

  function inkElement(pts: number[], size: number, color: string, hl: boolean, opacity = 1): InkEl {
    let minX = Infinity
    let minY = Infinity
    let maxX = -Infinity
    let maxY = -Infinity
    for (let i = 0; i < pts.length; i += 3) {
      minX = Math.min(minX, pts[i])
      minY = Math.min(minY, pts[i + 1])
      maxX = Math.max(maxX, pts[i])
      maxY = Math.max(maxY, pts[i + 1])
    }
    const rnd = strokeRound(size)
    const rel = pts.map((v, i) => (i % 3 === 0 ? rnd(v - minX) : i % 3 === 1 ? rnd(v - minY) : Math.round(v * 1000) / 1000))
    return { id: uid(), type: hl ? 'highlighter' : 'ink', x: minX, y: minY, w: maxX - minX, h: maxY - minY, points: rel, color, size, ...baseProps(), opacity }
  }

  function commitDraw(g: Extract<Gesture, { kind: 'draw' }>) {
    aw('live', null, true)
    // The filter trails the pen slightly: finish exactly where it was lifted.
    if (g.raw && g.pts.length) pushDrawPoint(g, g.raw, g.pts[g.pts.length - 1])
    if (!g.pts.length || readOnly()) return
    const pts = thinPoints(g.pts, 0.4 / cam().z)
    const prefs = st().prefs
    let el: El = inkElement(pts, g.size, g.color, g.hl, g.opacity)
    if (!g.hl && prefs.inkToShape && !g.snap) {
      const r = recognize(pts, 1 / cam().z)
      if (r?.kind === 'line') {
        el = lineElement(r.x1, r.y1, r.x2, r.y2, { stroke: g.color, strokeWidth: g.size, arrowEnd: false, opacity: g.opacity })
      } else if (r) {
        const base = { id: uid(), type: 'shape' as const, fill: 'transparent', stroke: g.color, strokeWidth: g.size, radius: 0, dash: false, ...baseProps(), opacity: g.opacity }
        if (r.kind === 'triangle') {
          const b = boundsOf(r.points)
          el = { ...base, shape: 'polygon', ...b, points: r.points.flatMap((p) => [r2((p.x - b.x) / b.w), r2((p.y - b.y) / b.h)]) }
        } else el = { ...base, shape: r.kind, x: r.x, y: r.y, w: r.w, h: r.h }
      }
    }
    board.undo.stopCapturing()
    board.add([el])
  }

  function lineElement(x1: number, y1: number, x2: number, y2: number, style: Partial<LineEl>): LineEl {
    const minX = Math.min(x1, x2)
    const minY = Math.min(y1, y2)
    return {
      id: uid(),
      type: 'line',
      x: minX,
      y: minY,
      w: Math.abs(x2 - x1),
      h: Math.abs(y2 - y1),
      points: [x1 - minX, y1 - minY, x2 - minX, y2 - minY],
      stroke: '#1E1E1E',
      strokeWidth: 3,
      dash: false,
      arrowStart: false,
      arrowEnd: false,
      ...baseProps(),
      ...style,
    }
  }

  /* ---------- erasing ---------- */

  function startErase(p: Pt, pointerId: number) {
    board.undo.stopCapturing()
    const g: Extract<Gesture, { kind: 'erase' }> = { kind: 'erase', last: p, pointerId, seg: 0, touched: new Map() }
    gesture = g
    eraseAlong(g, p, p)
  }

  function eraseAlong(g: Extract<Gesture, { kind: 'erase' }>, a: Pt, b: Pt) {
    const prefs = st().prefs.eraser
    const radius = prefs.size / 2 / cam().z
    if (prefs.mode === 'stroke') return eraseStrokes(a, b, radius)
    // Pixel eraser (Paint): each element touched gets this pass as a mark in its own coordinates.
    g.seg++
    const area = expand(normalizeBox(a.x, a.y, b.x, b.y), radius)
    const steps = Math.max(1, Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / Math.max(radius, 1e-6)))
    const rnd = strokeRound(radius)
    const strength = clamp(prefs.strength, 0.05, 1)
    for (const el of board.all()) {
      if (!PIXEL_ERASABLE.has(el.type) || !editable(el) || !intersects(box(el), area)) continue
      let hit = false
      for (let i = 0; i <= steps && !hit; i++) hit = hitTest(el, a.x + ((b.x - a.x) * i) / steps, a.y + ((b.y - a.y) * i) / steps, radius, true)
      if (!hit) continue
      let t = g.touched.get(el.id)
      if (!t) g.touched.set(el.id, (t = { orig: el, strokes: [], lastSeg: -1 }))
      const pa = toLocal(t.orig, a.x, a.y)
      const pb = toLocal(t.orig, b.x, b.y)
      const run = t.lastSeg === g.seg - 1 ? t.strokes[t.strokes.length - 1] : undefined
      if (run) {
        // Points closer than a quarter of the eraser add nothing visible.
        if (Math.hypot(pb.x - run[run.length - 2], pb.y - run[run.length - 1]) > radius / 4) run.push(rnd(pb.x), rnd(pb.y))
      } else t.strokes.push([rnd(pa.x), rnd(pa.y), rnd(pb.x), rnd(pb.y)])
      t.lastSeg = g.seg
      const s = rnd(radius * 2)
      preview.set(el.id, { ...t.orig, erase: [...(t.orig.erase ?? []), ...t.strokes.map((p) => ({ p, s, a: strength }))] })
    }
    scheduleFlush()
    invalidate(true)
  }

  /** End of a pixel-eraser pass: elements with nothing visible left are removed for good. */
  function finishErase(g: Extract<Gesture, { kind: 'erase' }>) {
    clearTimeout(flushTimer)
    flushTimer = 0
    if (!g.touched.size || readOnly()) return preview.clear()
    board.transact(() => {
      for (const id of g.touched.keys()) {
        const el = preview.get(id)
        if (!el || !board.elements.has(id)) continue
        if (nothingLeft(el)) board.elements.delete(id)
        else board.elements.set(id, el)
      }
    })
    preview.clear()
  }

  /** Whether the eraser marks hide the whole element: drawn small, is any pixel still there? */
  function nothingLeft(el: El) {
    if (!el.erase?.some((m) => m.a >= 0.99)) return false
    const b = box(el)
    const k = 160 / Math.max(b.w, b.h, 1e-6)
    const w = Math.max(1, Math.ceil(b.w * k))
    const h = Math.max(1, Math.ceil(b.h * k))
    const c = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(w, h) : Object.assign(document.createElement('canvas'), { width: w, height: h })
    const ctx = c.getContext('2d') as CanvasRenderingContext2D | null
    if (!ctx) return false
    ctx.setTransform(k, 0, 0, k, -b.x * k, -b.y * k)
    drawElement(ctx, { ...el, opacity: 1 }, { images })
    const data = ctx.getImageData(0, 0, w, h).data
    for (let i = 3; i < data.length; i += 4) if (data[i] > 24) return false
    return true
  }

  /** Stroke mode: whatever the eraser touches goes away whole. */
  function eraseStrokes(a: Pt, b: Pt, radius: number) {
    const area = expand(normalizeBox(a.x, a.y, b.x, b.y), radius)
    const remove: string[] = []
    for (const el of board.all()) {
      if (!ERASABLE.has(el.type) || !editable(el) || !intersects(box(el), area)) continue
      const la = toLocal(el, a.x, a.y)
      const lb = toLocal(el, b.x, b.y)
      if (el.type === 'ink' || el.type === 'highlighter') {
        const reach = radius + el.size / 2
        const pts = el.points
        let any = false
        for (let i = 0; i < pts.length && !any; i += 3) any = distToSegment(pts[i], pts[i + 1], la.x, la.y, lb.x, lb.y) <= reach
        // A fast swipe can cross a stroke between two of its points.
        for (let i = 0; i + 3 < pts.length && !any; i += 3) any = segmentDistance(pts[i], pts[i + 1], pts[i + 3], pts[i + 4], la.x, la.y, lb.x, lb.y) <= reach
        if (any) remove.push(el.id)
      } else if (el.type === 'line') {
        if (segmentDistance(el.points[0], el.points[1], el.points[2], el.points[3], la.x, la.y, lb.x, lb.y) <= radius + el.strokeWidth / 2) remove.push(el.id)
      } else {
        const steps = Math.max(1, Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / Math.max(radius, 1)))
        for (let i = 0; i <= steps; i++) {
          const t = i / steps
          if (hitTest(el, a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, radius)) {
            remove.push(el.id)
            break
          }
        }
      }
    }
    if (remove.length) board.remove(remove)
  }

  /* ---------- gestures ---------- */

  function startDraw(e: PointerEvent, s: Pt, hl: boolean) {
    const state = st()
    const pen = hl ? state.prefs.highlighter : state.prefs.pens[state.pen] ?? state.prefs.pens[0]
    const size = worldSize(pen.size)
    gesture = {
      kind: 'draw',
      hl,
      color: pen.color,
      size,
      opacity: clamp((pen as { opacity?: number }).opacity ?? 1, 0.05, 1),
      pts: [],
      snap: rulerSnapper(state.ruler, s.x, s.y, (size * cam().z) / 2),
      pointerId: e.pointerId,
      filter: new OneEuro2D(SMOOTHING_CUTOFF[state.prefs.inkSmoothing] ?? SMOOTHING_CUTOFF.medium),
      pressure: null,
      usePressure: e.pointerType === 'pen' && state.prefs.pressure && !hl,
      raw: null,
    }
    addDrawPoints(e)
  }

  function pushDrawPoint(g: Extract<Gesture, { kind: 'draw' }>, s: Pt, pressure: number, force = false) {
    const c = cam()
    if (g.snap) s = g.snap(s.x, s.y)
    const w = toWorld(c, s.x, s.y)
    const n = g.pts.length
    if (!force && n && Math.hypot(g.pts[n - 3] - w.x, g.pts[n - 2] - w.y) < 0.5 / c.z) return
    g.pts.push(w.x, w.y, pressure)
  }

  function addDrawPoints(e: PointerEvent) {
    const g = gesture
    if (g?.kind !== 'draw') return
    const evs = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : []
    for (const ev of evs.length ? evs : [e]) {
      const raw = screenPt(ev)
      g.raw = raw
      const s = g.filter.filter(raw.x, raw.y, ev.timeStamp)
      let pressure = -1
      if (g.usePressure) {
        g.pressure = nextPressure(g.pressure, ev.pressure)
        pressure = storedPressure(g.pressure)
      }
      pushDrawPoint(g, s, pressure)
    }
    const rnd = strokeRound(g.size)
    aw('live', { pts: g.pts.slice(-3000).map((v, i) => (i % 3 === 2 ? r2(v) : rnd(v))), color: g.color, size: g.size, hl: g.hl, op: g.opacity })
    invalidate(false)
  }

  /** The elements plus everything inside the sections among them (a section moves with its content). */
  function withContent(els: El[]): El[] {
    const out = new Map(els.map((el) => [el.id, el]))
    for (const sec of els) {
      if (sec.type !== 'section') continue
      const f = frameBox(sec)
      for (const el of board.all()) if (!out.has(el.id) && editable(el) && inside(f, frameBox(el))) out.set(el.id, el)
    }
    return [...out.values()]
  }

  /**
   * Copies with new ids, stacked on top in the same order. Connectors copied with both ends' elements
   * join the copies; an end whose element stays behind is let go.
   */
  function cloneAll(els: El[], dx: number, dy: number): { clones: El[]; ids: Map<string, string> } {
    const ids = new Map(els.map((el) => [el.id, uid()]))
    let z = board.topZ()
    const clones = [...els]
      .sort((a, b) => a.z - b.z)
      .map((el) => {
        let c = { ...el, id: ids.get(el.id)!, x: el.x + dx, y: el.y + dy, z: z++, locked: false } as El
        if (c.type === 'line') {
          const lose = (['from', 'to'] as const).filter((k) => c.type === 'line' && c[k] && !ids.has(c[k]!))
          c = unbind(c, lose)
          if (c.from) c.from = ids.get(c.from)
          if (c.to) c.to = ids.get(c.to)
        }
        return c
      })
    return { clones, ids }
  }

  function startMove(p: Pt, s: Pt, duplicate: boolean) {
    const picked = st().selection.map((id) => board.get(id)).filter((el): el is El => !!el && editable(el))
    if (!picked.length) return
    let els = withContent(picked)
    board.undo.stopCapturing()
    if (duplicate) {
      const { clones, ids } = cloneAll(els, 0, 0)
      board.add(clones)
      els = clones
      set({ selection: picked.map((el) => ids.get(el.id)!) })
    }
    const ids = els.map((el) => el.id)
    const moving = new Set(ids)
    // A connector dragged on its own lets go of the elements it joined.
    const orig = new Map<string, El>(
      els.map((el): [string, El] => {
        if (el.type !== 'line') return [el.id, el]
        const lose = (['from', 'to'] as const).filter((k) => el[k] && !moving.has(el[k]!))
        return [el.id, lose.length ? unbind(el, lose) : el]
      }),
    )
    const v = expand(view(), 200 / cam().z)
    const cands: Box[] = []
    for (const el of board.all()) {
      if (orig.has(el.id) || el.hidden || !intersects(box(el), v)) continue
      cands.push(frameBox(el))
      if (cands.length > 400) break
    }
    gesture = { kind: 'move', start: p, orig, box: union([...orig.values()].map(frameBox))!, cands, moved: false, sx: s.x, sy: s.y }
  }

  function snapMove(b: Box, cands: Box[]): { dx: number; dy: number } {
    const tol = 6 / cam().z
    let bestX = { d: tol, t: 0, c: null as Box | null }
    let bestY = { d: tol, t: 0, c: null as Box | null }
    const xs = (r: Box) => [r.x, r.x + r.w / 2, r.x + r.w]
    const ys = (r: Box) => [r.y, r.y + r.h / 2, r.y + r.h]
    let dx = 0
    let dy = 0
    for (const c of cands) {
      for (const a of xs(b))
        for (const t of xs(c))
          if (Math.abs(t - a) < Math.abs(bestX.d)) {
            bestX = { d: t - a, t, c }
          }
      for (const a of ys(b))
        for (const t of ys(c))
          if (Math.abs(t - a) < Math.abs(bestY.d)) {
            bestY = { d: t - a, t, c }
          }
    }
    guides = []
    if (bestX.c) {
      dx = bestX.d
      const c = bestX.c
      guides.push({ x1: bestX.t, y1: Math.min(b.y, c.y), x2: bestX.t, y2: Math.max(b.y + b.h, c.y + c.h) })
    }
    if (bestY.c) {
      dy = bestY.d
      const c = bestY.c
      guides.push({ x1: Math.min(b.x + dx, c.x), y1: bestY.t, x2: Math.max(b.x + dx + b.w, c.x + c.w), y2: bestY.t })
    }
    return { dx, dy }
  }

  function startCreate(tool: Tool, p: Pt) {
    const prefs = st().prefs
    const style = { stroke: prefs.shapeStyle.stroke, strokeWidth: worldSize(prefs.shapeStyle.strokeWidth) }
    let el: El
    if (tool === 'section') {
      const n = board.all().filter((e) => e.type === 'section').length + 1
      el = { id: uid(), type: 'section', name: `Sezione ${n}`, x: p.x, y: p.y, w: 0, h: 0, fill: SECTION_COLORS[0], ...baseProps(), z: board.bottomZ() } satisfies SectionEl
    } else if (tool === 'shape') {
      const round = prefs.lastShape === 'roundRect'
      el = {
        id: uid(),
        type: 'shape',
        shape: round ? 'rect' : (prefs.lastShape as ShapeKind),
        x: p.x,
        y: p.y,
        w: 0,
        h: 0,
        fill: prefs.shapeStyle.fill,
        radius: round ? worldSize(16) : 0,
        dash: false,
        ...style,
        ...baseProps(),
      } satisfies ShapeEl
    } else if (tool === 'tape') {
      el = lineElement(p.x, p.y, p.x, p.y, { stroke: prefs.tape.color, strokeWidth: worldSize(prefs.tape.size), tape: true })
    } else {
      // A connector starting on a shape, sticky, text… is attached to it.
      const from = hitElement(p)
      el = lineElement(p.x, p.y, p.x, p.y, { ...style, arrowEnd: tool === 'arrow', ...(connectable(from) ? { from: from.id } : {}) })
    }
    gesture = { kind: 'create', el, start: p, moved: false }
  }

  /** Connector being drawn or re-attached: the element under the pointer it would join (outlined). */
  function connectTarget(p: Pt, not?: string): El | null {
    const hit = hitElement(p, 8)
    const target = connectable(hit) && hit.id !== not ? hit : null
    if ((target?.id ?? null) !== hoverId) hoverId = target?.id ?? null
    return target
  }

  function updateCreate(g: Extract<Gesture, { kind: 'create' }>, p: Pt, e: PointerEvent) {
    g.moved = true
    if (g.el.type === 'line') {
      let end = p
      if (e.shiftKey) {
        const a = snapAngle(Math.atan2(p.y - g.start.y, p.x - g.start.x), Math.PI / 4)
        const len = Math.hypot(p.x - g.start.x, p.y - g.start.y)
        end = { x: g.start.x + Math.cos(a) * len, y: g.start.y + Math.sin(a) * len }
      }
      if (g.el.tape) g.el = setLineEnds(g.el, g.start, end)
      else {
        const target = connectTarget(p, g.el.from)
        const line = unbind(setLineEnds(g.el, g.start, end), ['to'])
        if (target) line.to = target.id
        g.el = routeConnector(line, current) ?? line
      }
    } else {
      let dx = p.x - g.start.x
      let dy = p.y - g.start.y
      if (e.shiftKey) {
        const m = Math.max(Math.abs(dx), Math.abs(dy))
        dx = Math.sign(dx || 1) * m
        dy = Math.sign(dy || 1) * m
      }
      const b = e.altKey ? normalizeBox(g.start.x - dx, g.start.y - dy, g.start.x + dx, g.start.y + dy) : normalizeBox(g.start.x, g.start.y, g.start.x + dx, g.start.y + dy)
      g.el = { ...g.el, ...b }
    }
    invalidate(true)
  }

  function finishCreate(g: Extract<Gesture, { kind: 'create' }>) {
    let el = g.el
    const z = cam().z
    hoverId = null
    if (g.spawn) return finishSpawn(g)
    if (!g.moved || (el.w * z < 4 && el.h * z < 4)) {
      if (el.type === 'line') el = unbind(setLineEnds(el, { x: g.start.x - 80 / z, y: g.start.y }, { x: g.start.x + 80 / z, y: g.start.y }), ['from', 'to'])
      else if (el.type === 'section') el = { ...el, x: g.start.x - 320 / z, y: g.start.y - 200 / z, w: 640 / z, h: 400 / z }
      else el = { ...el, x: g.start.x - 60 / z, y: g.start.y - 60 / z, w: 120 / z, h: 120 / z }
    }
    board.undo.stopCapturing()
    board.add([el])
    set({ selection: [el.id], tool: 'select', ...(el.type === 'section' && !g.moved ? { editingId: el.id } : {}) })
  }

  /** Copy of a shape or sticky for the "+" handles: same look, no text, on top. */
  function sibling(el: El, cx: number, cy: number): El {
    const copy = { ...el, id: uid(), x: cx - el.w / 2, y: cy - el.h / 2, z: board.topZ(), locked: false } as El
    delete copy.erase
    if (copy.type === 'sticky') Object.assign(copy, { text: '', author: myName() })
    if (copy.type === 'shape') delete copy.text
    return copy
  }

  /** Released after dragging from a "+": joined to what is under the pointer, or to a new copy there. */
  function finishSpawn(g: Extract<Gesture, { kind: 'create' }>) {
    const src = g.spawn!
    let line = g.el as LineEl
    const add: El[] = []
    if (!line.to) {
      const end = lineWorldEnds(line)[1]
      const copy = sibling(src, end.x, end.y)
      add.push(copy)
      line = routeConnector({ ...line, to: copy.id }, (id) => (id === copy.id ? copy : current(id))) ?? line
    }
    board.undo.stopCapturing()
    board.add([...add, line])
    set({ selection: [add[0]?.id ?? line.id], tool: 'select', ...(add[0]?.type === 'sticky' ? { editingId: add[0].id } : {}) })
  }

  /** Click on a "+": a connected copy beside the element, on that side. */
  function addBeside(el: El, side: AddHandle) {
    const z = cam().z
    const gap = Math.max(80 / z, Math.min(el.w, el.h) * 0.5)
    const c = center(el)
    const dir = { 'add-n': [0, -1], 'add-e': [1, 0], 'add-s': [0, 1], 'add-w': [-1, 0] }[side]
    const copy = sibling(el, c.x + dir[0] * (el.w + gap), c.y + dir[1] * (el.h + gap))
    const prefs = st().prefs
    const base = lineElement(c.x, c.y, c.x, c.y, { stroke: prefs.shapeStyle.stroke, strokeWidth: worldSize(Math.max(2, prefs.shapeStyle.strokeWidth)), arrowEnd: true, from: el.id, to: copy.id })
    const line = routeConnector(base, (id) => (id === copy.id ? copy : current(id))) ?? base
    board.undo.stopCapturing()
    board.add([copy, line])
    set({ selection: [copy.id], ...(copy.type === 'sticky' ? { editingId: copy.id } : {}) })
  }

  function startText(p: Pt) {
    const t = st().prefs.text
    const fontSize = worldSize(t.fontSize)
    const el: TextEl = {
      id: uid(),
      type: 'text',
      x: p.x,
      y: p.y - (fontSize * LINE_HEIGHT) / 2,
      w: fontSize / 6,
      h: fontSize * LINE_HEIGHT,
      text: '',
      color: t.color,
      fontSize,
      font: t.font,
      align: 'left',
      bold: false,
      italic: false,
      fixedWidth: false,
      ...baseProps(),
    }
    board.undo.stopCapturing()
    board.add([el])
    set({ tool: 'select', selection: [el.id], editingId: el.id })
  }

  /** Name to sign stickies with; none for a host who never set one (they appear as "Proprietario"). */
  const myName = () => {
    const n = awareness?.getLocalState()?.user?.name
    if (typeof n !== 'string' || !n.trim() || (n === 'Proprietario' && !st().prefs.name.trim())) return undefined
    return n.trim().slice(0, 40)
  }

  function startSticky(p: Pt) {
    const s = worldSize(220)
    const el: StickyEl = { id: uid(), type: 'sticky', x: p.x - s / 2, y: p.y - s / 2, w: s, h: s, text: '', color: st().prefs.stickyColor, font: 'sans', align: 'center', author: myName(), ...baseProps() }
    board.undo.stopCapturing()
    board.add([el])
    set({ tool: 'select', selection: [el.id], editingId: el.id })
  }

  function placeStamp(p: Pt) {
    const s = worldSize(64)
    board.undo.stopCapturing()
    board.add([{ id: uid(), type: 'stamp', emoji: st().prefs.stamp, x: p.x - s / 2, y: p.y - s / 2, w: s, h: s, ...baseProps() }])
  }

  function resizeTo(g: Extract<Gesture, { kind: 'resize' }>, p: Pt, e: PointerEvent) {
    const f = g.frame
    const cx = f.x + f.w / 2
    const cy = f.y + f.h / 2
    const lp = rotate(p.x, p.y, cx, cy, -g.rotation)
    const l0 = rotate(g.start.x, g.start.y, cx, cy, -g.rotation)
    const dx = lp.x - l0.x
    const dy = lp.y - l0.y
    const h = g.handle
    let left = f.x
    let right = f.x + f.w
    let top = f.y
    let bottom = f.y + f.h
    const sym = e.altKey
    if (h.includes('w')) {
      left += dx
      if (sym) right -= dx
    }
    if (h.includes('e')) {
      right += dx
      if (sym) left -= dx
    }
    if (h.includes('n')) {
      top += dy
      if (sym) bottom -= dy
    }
    if (h.includes('s')) {
      bottom += dy
      if (sym) top -= dy
    }
    const els = [...g.orig.values()]
    const lockRatio = e.shiftKey || els.some((el) => el.type === 'image' || el.type === 'stamp') || (els.length === 1 && els[0].type === 'text' && h.length === 2)
    const minSize = 1 / cam().z
    let w = Math.max(minSize, right - left)
    let hh = Math.max(minSize, bottom - top)
    if (right - left < minSize) h.includes('w') ? (left = right - minSize) : (right = left + minSize)
    if (bottom - top < minSize) h.includes('n') ? (top = bottom - minSize) : (bottom = top + minSize)
    if (lockRatio && f.w > 0 && f.h > 0) {
      const ratio = f.w / f.h
      if (h.length === 2) {
        if (w / hh > ratio) hh = w / ratio
        else w = hh * ratio
      } else if (h === 'n' || h === 's') w = hh * ratio
      else hh = w / ratio
      // Re-anchor on the side opposite the dragged handle.
      if (sym) {
        left = cx - w / 2
        top = cy - hh / 2
      } else {
        left = h.includes('w') ? f.x + f.w - w : h.includes('e') ? f.x : cx - w / 2
        top = h.includes('n') ? f.y + f.h - hh : h.includes('s') ? f.y : cy - hh / 2
      }
    }
    // New box in the unrotated frame; bring its centre back to world space.
    const nc = rotate(left + w / 2, top + hh / 2, cx, cy, g.rotation)
    const to: Box = { x: nc.x - w / 2, y: nc.y - hh / 2, w, h: hh }
    preview.clear()
    for (const el of els) {
      if (els.length === 1) {
        const base = { ...el, rotation: 0 } as El
        const patch = scaleElement(base, f, to)
        let next = { ...el, ...patch, rotation: el.rotation } as El
        if (next.type === 'text' && el.type === 'text') next = resizeText(el, next, h)
        preview.set(el.id, next)
      } else {
        let next = { ...el, ...scaleElement(el, f, to) } as El
        if (next.type === 'text' && el.type === 'text') next = { ...next, fontSize: Math.max(MIN_SIZE, el.fontSize * Math.sqrt((to.w / f.w) * (to.h / f.h))) } as TextEl
        preview.set(el.id, next)
      }
    }
    followConnectors(g.orig.keys())
    scheduleFlush()
    invalidate(true)
  }

  /** Connectors attached to elements being changed live follow them on screen right away. */
  function followConnectors(ids: Iterable<string>) {
    const changed = new Set(ids)
    for (const el of board.all()) {
      if (el.type !== 'line' || changed.has(el.id) || !((el.from && changed.has(el.from)) || (el.to && changed.has(el.to)))) continue
      const next = routeConnector(el, current)
      if (next) preview.set(el.id, next)
    }
  }

  /** Text: corner handles scale the type, side handles set a wrapping width. */
  function resizeText(orig: TextEl, next: TextEl, h: Handle): TextEl {
    if (h.length === 2) {
      const k = next.w / Math.max(orig.w, 0.01)
      const fontSize = Math.max(MIN_SIZE, orig.fontSize * k)
      const layout = layoutText(orig.text || ' ', fontCss(orig.font, fontSize, orig.bold, orig.italic), fontSize, orig.fixedWidth ? orig.w * k : null)
      return { ...next, fontSize, h: layout.height, w: orig.fixedWidth ? orig.w * k : layout.width }
    }
    if (h === 'e' || h === 'w') {
      const layout = layoutText(orig.text || ' ', fontCss(orig.font, orig.fontSize, orig.bold, orig.italic), orig.fontSize, next.w)
      return { ...next, fixedWidth: true, h: layout.height, y: orig.y + (next.y - orig.y) * 0 }
    }
    return { ...orig }
  }

  function rotateTo(g: Extract<Gesture, { kind: 'rotate' }>, p: Pt, e: PointerEvent) {
    let delta = Math.atan2(p.y - g.c.y, p.x - g.c.x) - g.a0
    const els = [...g.orig.values()]
    if (e.shiftKey || els.length === 1) {
      const base = els.length === 1 ? els[0].rotation : 0
      const step = e.shiftKey ? Math.PI / 12 : Math.PI / 180
      let target = snapAngle(base + delta, step)
      // Gently stick to right angles.
      const right = snapAngle(target, Math.PI / 2)
      if (!e.shiftKey && Math.abs(right - target) < (2 * Math.PI) / 180) target = right
      delta = target - base
    }
    preview.clear()
    for (const el of els) {
      const c = center(el)
      const nc = rotate(c.x, c.y, g.c.x, g.c.y, delta)
      if (el.type === 'line') {
        const [a, b] = lineWorldEnds(el)
        preview.set(el.id, setLineEnds(el, rotate(a.x, a.y, g.c.x, g.c.y, delta), rotate(b.x, b.y, g.c.x, g.c.y, delta)))
      } else preview.set(el.id, { ...el, x: nc.x - el.w / 2, y: nc.y - el.h / 2, rotation: el.rotation + delta })
    }
    followConnectors(g.orig.keys())
    scheduleFlush()
    invalidate(true)
  }

  function finishGesture() {
    flushPreview()
    preview.clear()
    guides = []
    hoverId = null
    invalidate(true)
  }

  /* ---------- pointer events ---------- */

  const touches = () => [...pointers.entries()].filter(([, p]) => p.type === 'touch')

  function startPinch() {
    const t = touches()
    if (t.length < 2) return
    const [[ia, a], [ib, b]] = t
    const r = st().ruler
    const onR = onRuler(r, a.x, a.y) && onRuler(r, b.x, b.y)
    gesture = {
      kind: 'pinch',
      ids: [ia, ib],
      d0: Math.hypot(b.x - a.x, b.y - a.y) || 1,
      c0: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 },
      cam: cam(),
      ruler: onR ? { x: r.x, y: r.y, angle: r.angle, a0: Math.atan2(b.y - a.y, b.x - a.x) } : null,
    }
    stopFollowing()
  }

  function onPointerDown(e: PointerEvent) {
    const now = performance.now()
    if (e.pointerType === 'pen') lastPen = now
    // Palm rejection: ignore the hand resting on the screen while the pen is in use.
    if (e.pointerType === 'touch' && now - lastPen < 1200) return
    if (e.pointerType === 'mouse' && e.button === 2) return // context menu
    // Without this the browser's follow-up mousedown moves focus to <body>, which instantly
    // closed a text box created by this very click.
    e.preventDefault()
    const s = screenPt(e)
    pointers.set(e.pointerId, { x: s.x, y: s.y, type: e.pointerType })
    try {
      overlay.setPointerCapture(e.pointerId)
    } catch {
      /* pointer already gone */
    }
    if (e.pointerType === 'touch' && touches().length === 2) {
      // Second finger: cancel a stroke that just started and pinch instead.
      if (gesture?.kind === 'draw') aw('live', null, true)
      if (gesture && gesture.kind !== 'pinch') finishGesture()
      startPinch()
      return
    }
    if (gesture) return
    // Leave any text field (inspector, layer name, text box) before acting on the canvas.
    const active = document.activeElement as HTMLElement | null
    if (active && active !== document.body) active.blur()
    if (st().editingId) set({ editingId: null })
    if (st().commentId) set({ commentId: null })

    const state = st()
    const p = toWorld(cam(), s.x, s.y)
    const ro = state.readOnly
    let tool: Tool = state.tool
    if (ro && !['select', 'hand', 'laser'].includes(tool)) tool = 'hand'
    const touchPans = e.pointerType === 'touch' && !state.prefs.fingerDraw

    // Pen eraser end and barrel button.
    if (!ro && e.pointerType === 'pen' && (e.button === 5 || (e.buttons & 32) !== 0)) {
      startErase(p, e.pointerId)
      return
    }
    if (!ro && e.pointerType === 'pen' && e.button === 2) {
      gesture = { kind: 'lasso', poly: [p.x, p.y], tap: { clientX: e.clientX, clientY: e.clientY } }
      return
    }

    if (e.button === 1 || spaceDown || tool === 'hand') {
      gesture = { kind: 'pan', sx: s.x, sy: s.y, cam: cam() }
      stopFollowing()
      updateCursor()
      return
    }

    // Ruler: draw along its edge with ink tools, otherwise drag or rotate it.
    const r = state.ruler
    if (r.visible) {
      const inkTool = (tool === 'pen' || tool === 'highlighter') && !touchPans
      if (!(inkTool && rulerSnapper(r, s.x, s.y, 2))) {
        const knob = { x: r.x + Math.cos(r.angle) * (RULER_LENGTH / 2 - 26), y: r.y + Math.sin(r.angle) * (RULER_LENGTH / 2 - 26) }
        if (Math.hypot(s.x - knob.x, s.y - knob.y) < 18) {
          gesture = { kind: 'ruler-rotate' }
          return
        }
        if (onRuler(r, s.x, s.y)) {
          gesture = { kind: 'ruler', sx: s.x, sy: s.y, x: r.x, y: r.y }
          return
        }
      }
    }

    const handle = !ro && (tool === 'select' || tool === 'lasso') ? hitHandle(s, e.pointerType !== 'mouse') : null
    if (handle) {
      board.undo.stopCapturing()
      const sel = selected()
      const orig = new Map(sel.map((el) => [el.id, el]))
      if (handle.startsWith('add-')) {
        // Drag: a connector from this element; a click (no drag) adds a connected copy on that side.
        const src = sel[0]
        const prefs = st().prefs
        const el = lineElement(p.x, p.y, p.x, p.y, { stroke: prefs.shapeStyle.stroke, strokeWidth: worldSize(Math.max(2, prefs.shapeStyle.strokeWidth)), arrowEnd: true, from: src.id })
        gesture = { kind: 'create', el, start: p, moved: false, spawn: src }
        pendingAdd = handle as AddHandle
      } else if (handle === 'p0' || handle === 'p1') gesture = { kind: 'endpoint', which: handle === 'p0' ? 0 : 1, orig: sel[0] as LineEl }
      else if (handle === 'rot') {
        const f = selectionFrame()!
        const c = center(f.box)
        gesture = { kind: 'rotate', c, a0: Math.atan2(p.y - c.y, p.x - c.x), orig }
      } else {
        const f = selectionFrame()!
        gesture = { kind: 'resize', handle, start: p, orig, frame: f.box, rotation: f.rotation }
      }
      return
    }

    // Voting session: a click puts one of your votes on what is under it (Maiusc or Alt takes it back).
    const voting = board.voting()
    if (voting && !voting.ended && !ro && tool === 'select') {
      const hit = hitElement(p)
      if (hit && votable(hit)) {
        board.castVote(voterId(), hit.id, e.shiftKey || e.altKey ? -1 : 1)
        return
      }
    }

    switch (tool) {
      case 'select':
      case 'lasso': {
        if (tool === 'lasso' && !insideSelection(p)) {
          if (touchPans) gesture = { kind: 'pan', sx: s.x, sy: s.y, cam: cam() }
          else gesture = { kind: 'lasso', poly: [p.x, p.y] }
          return
        }
        const hit = tool === 'lasso' ? null : hitElement(p, e.pointerType === 'mouse' ? 4 : 10)
        const sel = st().selection
        if (hit) {
          if (e.shiftKey) {
            set({ selection: sel.includes(hit.id) ? sel.filter((id) => id !== hit.id) : [...sel, hit.id] })
            if (sel.includes(hit.id)) return
          } else if (!sel.includes(hit.id)) set({ selection: [hit.id] })
          if (!ro) startMove(p, s, e.altKey)
          return
        }
        if (tool === 'lasso' || (insideSelection(p) && selected().length > 1)) {
          if (!ro) startMove(p, s, e.altKey)
          return
        }
        if (touchPans) {
          set({ selection: [] })
          gesture = { kind: 'pan', sx: s.x, sy: s.y, cam: cam() }
          return
        }
        gesture = { kind: 'marquee', start: p, cur: p, base: e.shiftKey ? sel : [] }
        if (!e.shiftKey) set({ selection: [] })
        return
      }
      case 'pen':
      case 'highlighter':
        if (touchPans) break
        startDraw(e, s, tool === 'highlighter')
        return
      case 'eraser':
        if (touchPans) break
        startErase(p, e.pointerId)
        return
      case 'shape':
      case 'line':
      case 'arrow':
      case 'section':
      case 'tape':
        if (touchPans) break
        startCreate(tool, p)
        return
      case 'comment': {
        // On a pin: open its thread. Elsewhere: a new pin, its thread open to write the first message.
        const hit = hitElement(p)
        if (hit?.type === 'comment') {
          set({ selection: [hit.id], commentId: hit.id })
          return
        }
        const el: CommentEl = { id: uid(), type: 'comment', x: p.x, y: p.y, w: 0, h: 0, thread: [], ...baseProps() }
        board.undo.stopCapturing()
        board.add([el])
        set({ selection: [el.id], commentId: el.id, tool: 'select' })
        return
      }
      case 'text': {
        const hit = hitElement(p)
        if (hit?.type === 'text') set({ selection: [hit.id], editingId: hit.id, tool: 'select' })
        else startText(p)
        return
      }
      case 'sticky':
        startSticky(p)
        return
      case 'stamp': {
        // Clicking an existing element picks it up instead of stacking another stamp on it.
        const hit = hitElement(p)
        if (hit) {
          set({ selection: [hit.id] })
          if (!ro) startMove(p, s, e.altKey)
          return
        }
        placeStamp(p)
        return
      }
      case 'laser':
        gesture = { kind: 'laser' }
        laser.push({ ...p, t: performance.now() })
        return
    }
    gesture = { kind: 'pan', sx: s.x, sy: s.y, cam: cam() }
  }

  function onPointerMove(e: PointerEvent) {
    if (press && (e.pointerId !== press.id || Math.hypot(e.clientX - press.x, e.clientY - press.y) > 8)) cancelPress()
    const s = screenPt(e)
    if (e.pointerType === 'pen') lastPen = performance.now()
    const tracked = pointers.get(e.pointerId)
    if (tracked) {
      tracked.x = s.x
      tracked.y = s.y
    }
    const p = toWorld(cam(), s.x, s.y)
    if (e.pointerType !== 'touch') aw('cursor', { x: r2(p.x), y: r2(p.y) })
    if (st().tool === 'eraser' || gesture?.kind === 'erase') {
      eraserAt = s
      invalidate(false)
    }
    const g = gesture
    if (!g) {
      if (e.pointerType !== 'touch') {
        const tool = st().tool
        const h = tool === 'select' || tool === 'lasso' ? hitHandle(s, e.pointerType === 'pen') : null
        const hov = tool === 'select' && !h ? hitElement(p, 4)?.id ?? null : null
        if (h !== hoverHandle || hov !== hoverId) {
          hoverHandle = h
          hoverId = hov
          updateCursor()
          invalidate(false)
        }
      }
      return
    }
    if (tracked === undefined && g.kind !== 'laser') return
    switch (g.kind) {
      case 'pan':
        setCam({ x: g.cam.x + s.x - g.sx, y: g.cam.y + s.y - g.sy, z: g.cam.z })
        return
      case 'pinch': {
        const a = pointers.get(g.ids[0])
        const b = pointers.get(g.ids[1])
        if (!a || !b) return
        const c = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
        if (g.ruler) {
          const angle = g.ruler.angle + Math.atan2(b.y - a.y, b.x - a.x) - g.ruler.a0
          st().setRuler({ x: g.ruler.x + c.x - g.c0.x, y: g.ruler.y + c.y - g.c0.y, angle })
          return
        }
        const z = clamp((g.cam.z * Math.hypot(b.x - a.x, b.y - a.y)) / g.d0, MIN_ZOOM, MAX_ZOOM)
        const w0 = toWorld(g.cam, g.c0.x, g.c0.y)
        setCam({ x: c.x - w0.x * z, y: c.y - w0.y * z, z })
        return
      }
      case 'draw':
        if (e.pointerId === g.pointerId) addDrawPoints(e)
        return
      case 'erase':
        eraseAlong(g, g.last, p)
        g.last = p
        return
      case 'lasso':
        g.poly.push(p.x, p.y)
        invalidate(false)
        return
      case 'marquee': {
        g.cur = p
        const m = normalizeBox(g.start.x, g.start.y, p.x, p.y)
        // Sections only when wholly inside, so a selection drawn within one doesn't pick it up.
        const hits = board.all().filter((el) => editableForSelect(el) && (el.type === 'section' ? inside(m, frameBox(el)) : intersects(frameBox(el), m)) && (m.w > 0 || m.h > 0))
        const next = [...new Set([...g.base, ...hits.map((el) => el.id)])]
        const prev = st().selection
        // Most moves don't change what's inside: skip the update that re-renders the panels.
        if (next.length !== prev.length || next.some((id, i) => id !== prev[i])) set({ selection: next })
        invalidate(false)
        return
      }
      case 'move': {
        if (!g.moved && Math.hypot(s.x - g.sx, s.y - g.sy) < 3) return
        g.moved = true
        let dx = p.x - g.start.x
        let dy = p.y - g.start.y
        if (e.shiftKey) Math.abs(dx) > Math.abs(dy) ? (dy = 0) : (dx = 0)
        guides = []
        if (!e.ctrlKey && !e.metaKey) {
          const snap = snapMove({ ...g.box, x: g.box.x + dx, y: g.box.y + dy }, g.cands)
          dx += snap.dx
          dy += snap.dy
        }
        for (const [id, el] of g.orig) {
          const moved = { ...el, x: el.x + dx, y: el.y + dy } as El
          if (moved.type === 'line' && el.type === 'line' && el.erase?.length) moved.erase = el.erase
          preview.set(id, moved)
        }
        followConnectors(g.orig.keys())
        scheduleFlush()
        invalidate(true)
        return
      }
      case 'resize':
        resizeTo(g, p, e)
        return
      case 'rotate':
        rotateTo(g, p, e)
        return
      case 'endpoint': {
        const [a, b] = lineWorldEnds(g.orig)
        const fixed = g.which === 0 ? b : a
        let q = p
        if (e.shiftKey) {
          const ang = snapAngle(Math.atan2(p.y - fixed.y, p.x - fixed.x), Math.PI / 4)
          const len = Math.hypot(p.x - fixed.x, p.y - fixed.y)
          q = { x: fixed.x + Math.cos(ang) * len, y: fixed.y + Math.sin(ang) * len }
        }
        // Over a shape, sticky… the end attaches to it; elsewhere it is let go.
        const end = g.which === 0 ? 'from' : 'to'
        const target = connectTarget(p, g.which === 0 ? g.orig.to : g.orig.from)
        let line = unbind(g.which === 0 ? setLineEnds(g.orig, q, b) : setLineEnds(g.orig, a, q), [end])
        if (target) {
          line[end] = target.id
          line = routeConnector(line, current) ?? line
        }
        preview.set(g.orig.id, line)
        scheduleFlush()
        invalidate(true)
        return
      }
      case 'create':
        if (g.spawn && !g.moved && Math.hypot(s.x - toScreen(cam(), g.start.x, g.start.y).x, s.y - toScreen(cam(), g.start.x, g.start.y).y) < 4) return
        pendingAdd = null
        updateCreate(g, p, e)
        return
      case 'laser':
        laser.push({ ...p, t: performance.now() })
        if (laser.length > 80) laser.shift()
        aw('laser', laser.flatMap((q) => [r2(q.x), r2(q.y)]))
        invalidate(false)
        return
      case 'ruler': {
        st().setRuler({ x: g.x + s.x - g.sx, y: g.y + s.y - g.sy })
        return
      }
      case 'ruler-rotate': {
        const r = st().ruler
        let angle = Math.atan2(s.y - r.y, s.x - r.x)
        const right = snapAngle(angle, Math.PI / 4)
        if (Math.abs(right - angle) < (1.5 * Math.PI) / 180 || e.shiftKey) angle = e.shiftKey ? snapAngle(angle) : right
        st().setRuler({ angle })
        return
      }
    }
  }

  const editableForSelect = (el: El) => !el.locked && !el.hidden

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId)
    cancelPress()
    const g = gesture
    if (!g) return
    if (g.kind === 'pinch') {
      if (!g.ids.includes(e.pointerId)) return
      gesture = null
      return
    }
    if (g.kind === 'draw' && e.pointerId !== g.pointerId) return
    gesture = null
    switch (g.kind) {
      case 'draw':
        if (e.type === 'pointercancel') aw('live', null, true)
        else commitDraw(g)
        break
      case 'lasso': {
        const poly = g.poly
        if (g.tap && poly.every((v, i) => Math.abs(v - poly[i % 2]) * cam().z < 8)) {
          openMenu(g.tap)
          return
        }
        if (poly.length >= 6) {
          const ids = board
            .all()
            .filter((el) => {
              if (!editableForSelect(el)) return false
              const pts = samplePoints(el)
              const inside = pts.filter((q) => pointInPolygon(q.x, q.y, poly)).length
              return inside / pts.length >= 0.75
            })
            .map((el) => el.id)
          set({ selection: ids })
        }
        break
      }
      case 'create':
        if (pendingAdd && g.spawn) {
          const side = pendingAdd
          pendingAdd = null
          addBeside(g.spawn, side)
          break
        }
        finishCreate(g)
        break
      case 'erase':
        finishErase(g)
        break
      case 'laser':
        // Let the trail fade, then clear it for others.
        setTimeout(() => {
          if (gesture?.kind !== 'laser') aw('laser', null, true)
        }, 950)
        break
      case 'move':
        if (!g.moved && selected().length === 1 && selected()[0].type === 'comment') set({ commentId: selected()[0].id })
        if (!g.moved && !e.shiftKey && st().selection.length > 1) {
          // Click (no drag) inside a multi-selection picks the element under the pointer.
          const hit = hitElement(worldPt(e))
          if (hit) set({ selection: [hit.id] })
        }
        break
    }
    finishGesture()
    updateCursor()
  }

  /** Elements with text to type into: text, stickies, shapes (FigJam), section titles. */
  const writable = (el: El | null | undefined): el is El => !!el && !el.locked && (el.type === 'text' || el.type === 'sticky' || el.type === 'shape' || el.type === 'section')

  function onDoubleClick(e: MouseEvent) {
    if (readOnly()) return
    const hit = hitElement(worldPt(e))
    if (writable(hit)) set({ selection: [hit.id], editingId: hit.id, tool: 'select' })
  }

  function onPointerLeave(e: PointerEvent) {
    if (e.pointerType !== 'touch') aw('cursor', null)
    eraserAt = null
    hoverId = null
    invalidate(false)
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault()
    const s = screenPt(e)
    const r = st().ruler
    let dx = e.deltaX
    let dy = e.deltaY
    if (e.deltaMode === 1) {
      dx *= 16
      dy *= 16
    }
    if (r.visible && onRuler(r, s.x, s.y) && !e.ctrlKey) {
      st().setRuler({ angle: r.angle + (Math.sign(dy) * (e.shiftKey ? 15 : 1) * Math.PI) / 180 })
      return
    }
    stopFollowing()
    const c = cam()
    if (e.ctrlKey || e.metaKey || (st().prefs.wheel === 'zoom' && !e.shiftKey)) {
      zoomAt(s.x, s.y, c.z * Math.exp(-clamp(dy, -60, 60) * 0.0085))
      return
    }
    if (e.shiftKey && !dx) {
      dx = dy
      dy = 0
    }
    setCam({ x: c.x - dx, y: c.y - dy, z: c.z })
  }

  /* ---------- keyboard and clipboard ---------- */

  const typing = (t: EventTarget | null) => t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))

  const TOOL_KEYS: Record<string, Tool> = { v: 'select', h: 'hand', p: 'pen', m: 'highlighter', e: 'eraser', q: 'lasso', l: 'line', t: 'text', s: 'sticky', k: 'laser' }

  function onKeyDown(e: KeyboardEvent) {
    // Not from the text bar either: Backspace on its buttons must not delete the text being edited.
    if (typing(e.target) || (e.target instanceof Element && e.target.closest('[data-text-tools]')) || document.querySelector('[role="dialog"], [role="menu"], [role="listbox"]')) return
    const mod = e.ctrlKey || e.metaKey
    const key = e.key.toLowerCase()
    const state = st()
    const ro = state.readOnly

    if (e.key === ' ' && !spaceDown) {
      spaceDown = true
      updateCursor()
      e.preventDefault()
      return
    }
    if (mod) {
      const actions: Record<string, () => void> = {
        z: () => (e.shiftKey ? commands.redo() : commands.undo()),
        y: () => commands.redo(),
        d: () => commands.duplicate(),
        a: () => commands.selectAll(),
        '=': () => commands.zoomBy(1.25),
        '+': () => commands.zoomBy(1.25),
        '-': () => commands.zoomBy(0.8),
        '0': () => commands.zoomTo(1),
        ']': () => commands.order(e.shiftKey ? 'front' : 'forward'),
        '[': () => commands.order(e.shiftKey ? 'back' : 'backward'),
        '\\': toggleFocus,
      }
      actions.g = () => (e.shiftKey ? commands.ungroup() : commands.group())
      if (e.shiftKey && key === 'l') actions.l = () => commands.toggleLock()
      if (e.shiftKey && key === 'h') actions.h = () => commands.toggleHide()
      const fn = actions[key]
      if (fn && (!ro || ['=', '+', '-', '0', '\\', 'a'].includes(key))) {
        e.preventDefault()
        fn()
      }
      return
    }
    if (e.altKey) return
    if (e.shiftKey && (e.key === '!' || e.code === 'Digit1')) return commands.fit()
    if (e.shiftKey && (e.key === '@' || e.key === '"' || e.code === 'Digit2')) return commands.fitSelection()
    if (e.shiftKey && (e.key === ')' || e.key === '=' || e.code === 'Digit0')) return commands.zoomTo(1)
    if (e.key === 'Escape') {
      if (gesture) {
        if (gesture.kind === 'draw') aw('live', null, true)
        gesture = null
        preview.clear()
        guides = []
        invalidate(true)
      } else if (state.selection.length) set({ selection: [] })
      else state.setTool('select')
      return
    }
    if (ro) {
      if (key === 'v' || key === 'h' || key === 'k') state.setTool(TOOL_KEYS[key])
      return
    }
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault()
      return commands.remove()
    }
    if (e.key === 'Enter' && state.selection.length === 1) {
      const el = board.get(state.selection[0])
      if (el?.type === 'comment') return set({ commentId: el.id })
      if (writable(el)) {
        e.preventDefault()
        set({ editingId: el.id })
      }
      return
    }
    if (e.key.startsWith('Arrow') && state.selection.length) {
      e.preventDefault()
      const step = (e.shiftKey ? 10 : 1) / Math.max(1, cam().z)
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0
      board.updateMany(new Map(withContent(selected().filter(editable)).map((el) => [el.id, { x: el.x + dx, y: el.y + dy }])))
      return
    }
    if (e.shiftKey && key === 'l') return state.setTool('arrow')
    if (e.shiftKey && key === 's') return state.setTool('section')
    if (key === 'x') return state.setTool('arrow')
    if (key === 'c') return state.setTool('comment')
    if (key === 'w') return state.setTool('tape')
    if (e.key === '/') {
      e.preventDefault()
      return window.dispatchEvent(new CustomEvent('tratto:chat'))
    }
    if (/^[1-4]$/.test(e.key) && state.tool === 'pen') return set({ pen: Number(e.key) - 1 })
    if (key === 'r' || key === 'o') {
      state.setPrefs({ lastShape: key === 'r' ? 'rect' : 'ellipse' })
      return state.setTool('shape')
    }
    if (key === 'u') return state.setRuler({ visible: !state.ruler.visible, ...(state.ruler.visible ? {} : { x: W / 2, y: H / 2 }) })
    if (key === 'i') return window.dispatchEvent(new CustomEvent('tratto:insert-image'))
    if (!e.shiftKey && TOOL_KEYS[key]) state.setTool(TOOL_KEYS[key])
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === ' ') {
      spaceDown = false
      updateCursor()
    }
  }

  function onCopy(e: ClipboardEvent) {
    if (typing(e.target) || !st().selection.length) return
    e.preventDefault()
    clipboard = withContent(selected())
    pasteCount = 0
    e.clipboardData?.setData('text/plain', CLIP_PREFIX + JSON.stringify(clipboard))
  }

  function onCut(e: ClipboardEvent) {
    if (typing(e.target) || readOnly()) return
    onCopy(e)
    commands.remove()
  }

  function pasteElements(els: El[]) {
    if (!els.length || readOnly()) return
    pasteCount++
    const b = union(els.map(frameBox))!
    const v = view()
    // Paste in place, nudged; if the originals are off screen, paste in the middle of the view.
    let dx = (16 * pasteCount) / cam().z
    let dy = dx
    if (!intersects(b, v)) {
      dx = v.x + v.w / 2 - (b.x + b.w / 2)
      dy = v.y + v.h / 2 - (b.y + b.h / 2)
    }
    const { clones } = cloneAll(els, dx, dy)
    board.undo.stopCapturing()
    board.add(clones)
    set({ selection: clones.map((c) => c.id), tool: 'select' })
  }

  function onPaste(e: ClipboardEvent) {
    if (typing(e.target) || readOnly()) return
    const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith('image/'))
    if (files.length) {
      e.preventDefault()
      o.onImageFiles(files, commands.viewportCenter())
      return
    }
    const text = e.clipboardData?.getData('text/plain') ?? ''
    if (text.startsWith(CLIP_PREFIX)) {
      e.preventDefault()
      try {
        const els = JSON.parse(text.slice(CLIP_PREFIX.length))
        if (Array.isArray(els)) pasteElements(els)
      } catch {
        /* not ours after all */
      }
      return
    }
    if (text.trim()) {
      e.preventDefault()
      const c = commands.viewportCenter()
      const t = st().prefs.text
      const fontSize = worldSize(t.fontSize)
      const css = fontCss(t.font, fontSize)
      const layout = layoutText(text.slice(0, 20_000), css, fontSize, text.length > 80 ? 640 / cam().z : null)
      const el: TextEl = {
        id: uid(),
        type: 'text',
        x: c.x - layout.width / 2,
        y: c.y - layout.height / 2,
        w: layout.width,
        h: layout.height,
        text: text.slice(0, 20_000),
        color: t.color,
        fontSize,
        font: t.font,
        align: 'left',
        bold: false,
        italic: false,
        fixedWidth: text.length > 80,
        ...baseProps(),
      }
      board.undo.stopCapturing()
      board.add([el])
      set({ selection: [el.id], tool: 'select' })
    }
  }

  function onDrop(e: DragEvent) {
    e.preventDefault()
    if (readOnly()) return
    const files = [...(e.dataTransfer?.files ?? [])].filter((f) => f.type.startsWith('image/'))
    if (files.length) o.onImageFiles(files, worldPt(e))
  }
  const onDragOver = (e: DragEvent) => e.preventDefault()

  /** Right click acts on what is under the pointer, or on the selection if it's inside it. */
  function selectForMenu(e: { clientX: number; clientY: number }) {
    const p = worldPt(e)
    const hit = hitElement(p)
    const sel = st().selection
    if (hit && !sel.includes(hit.id)) set({ selection: [hit.id] })
    if (!hit && !insideSelection(p)) set({ selection: [] })
  }
  const onContextMenu = (e: MouseEvent) => selectForMenu(e)

  function openMenu(e: { clientX: number; clientY: number }) {
    gesture = null
    finishGesture()
    selectForMenu(e)
    o.onMenu(e.clientX, e.clientY)
  }

  // Pen or finger held still opens the menu, like a right click in Windows.
  let press: { id: number; x: number; y: number; timer: number } | null = null
  const cancelPress = () => {
    if (press) clearTimeout(press.timer)
    press = null
  }
  function startPress(e: PointerEvent) {
    cancelPress()
    const g = gesture
    // Not for the mouse (it has a right button), nor for a resting palm that palm rejection ignored.
    if (e.pointerType === 'mouse' || !pointers.has(e.pointerId) || !g || !['move', 'marquee', 'pan', 'lasso'].includes(g.kind)) return
    const { clientX, clientY, pointerId } = e
    press = {
      id: pointerId,
      x: clientX,
      y: clientY,
      timer: window.setTimeout(() => {
        press = null
        if (gesture === g && pointers.size === 1) openMenu({ clientX, clientY })
      }, 550),
    }
  }

  /* ---------- commands ---------- */

  const prune = () => {
    const sel = st().selection.filter((id) => board.get(id))
    if (sel.length !== st().selection.length) set({ selection: sel })
  }

  Object.assign(commands, {
    zoomBy: (f: number) => zoomAt(W / 2, H / 2, cam().z * f),
    zoomTo: (z: number) => zoomAt(W / 2, H / 2, z),
    fit: () => fitBox(union(board.all().filter((e) => !e.hidden).map(box)), 2),
    fitSelection: () => {
      const sel = selected()
      if (sel.length) fitBox(union(sel.map(box)), 4)
    },
    undo: () => {
      if (readOnly()) return
      board.undo.undo()
      prune()
    },
    redo: () => {
      if (readOnly()) return
      board.undo.redo()
      prune()
    },
    remove: () => {
      if (readOnly()) return
      const ids = selected().filter((el) => !el.locked).map((el) => el.id)
      if (!ids.length) return
      board.undo.stopCapturing()
      board.remove(ids)
      set({ selection: [] })
    },
    duplicate: () => {
      clipboard = withContent(selected())
      pasteCount = 0
      pasteElements(clipboard)
    },
    copy: () => {
      clipboard = withContent(selected())
      pasteCount = 0
      navigator.clipboard?.writeText(CLIP_PREFIX + JSON.stringify(clipboard)).catch(() => {})
    },
    cut: () => {
      commands.copy()
      commands.remove()
    },
    paste: () => pasteElements(clipboard),
    order: (dir: 'front' | 'back' | 'forward' | 'backward') => {
      if (readOnly()) return
      const sel = new Set(st().selection)
      if (!sel.size) return
      const all = board.all()
      const patches = new Map<string, Partial<El>>()
      if (dir === 'front' || dir === 'back') {
        let z = dir === 'front' ? board.topZ() : board.bottomZ() - sel.size
        for (const el of all) if (sel.has(el.id)) patches.set(el.id, { z: z++ })
      } else {
        // Swap with the nearest non-selected neighbour in stacking order.
        const order = [...all]
        const idx = (i: number) => (dir === 'forward' ? order.length - 1 - i : i)
        for (let k = 0; k < order.length; k++) {
          const i = idx(k)
          const j = dir === 'forward' ? i + 1 : i - 1
          if (sel.has(order[i].id) && j >= 0 && j < order.length && !sel.has(order[j].id)) {
            ;[order[i], order[j]] = [order[j], order[i]]
          }
        }
        order.forEach((el, i) => {
          if (el.z !== i) patches.set(el.id, { z: i })
        })
      }
      board.undo.stopCapturing()
      board.updateMany(patches)
    },
    toggleLock: () => {
      if (readOnly()) return
      const sel = selected()
      const lock = !sel.every((el) => el.locked)
      board.updateMany(new Map(sel.map((el) => [el.id, { locked: lock }])))
      if (lock) set({ selection: [] })
    },
    toggleHide: () => {
      if (readOnly()) return
      const sel = selected()
      const hide = !sel.every((el) => el.hidden)
      board.updateMany(new Map(sel.map((el) => [el.id, { hidden: hide }])))
      if (hide) set({ selection: [] })
    },
    selectAll: () => set({ selection: board.all().filter(editableForSelect).map((el) => el.id), tool: 'select' }),
    align: (kind: AlignKind) => {
      const sel = selected().filter(editable)
      if (sel.length < 2) return
      const b = union(sel.map(frameBox))!
      const patches = new Map<string, Partial<El>>()
      for (const el of sel) {
        const f = frameBox(el)
        let dx = 0
        let dy = 0
        if (kind === 'left') dx = b.x - f.x
        if (kind === 'right') dx = b.x + b.w - (f.x + f.w)
        if (kind === 'hcenter') dx = b.x + b.w / 2 - (f.x + f.w / 2)
        if (kind === 'top') dy = b.y - f.y
        if (kind === 'bottom') dy = b.y + b.h - (f.y + f.h)
        if (kind === 'vcenter') dy = b.y + b.h / 2 - (f.y + f.h / 2)
        patches.set(el.id, { x: el.x + dx, y: el.y + dy })
      }
      board.undo.stopCapturing()
      board.updateMany(patches)
    },
    distribute: (axis: 'x' | 'y') => {
      const sel = selected().filter(editable)
      if (sel.length < 3) return
      const items = sel.map((el) => ({ el, f: frameBox(el) })).sort((a, b) => (axis === 'x' ? a.f.x - b.f.x : a.f.y - b.f.y))
      const first = items[0].f
      const last = items[items.length - 1].f
      const span = axis === 'x' ? last.x + last.w - first.x : last.y + last.h - first.y
      const total = items.reduce((s, i) => s + (axis === 'x' ? i.f.w : i.f.h), 0)
      const gap = (span - total) / (items.length - 1)
      let pos = axis === 'x' ? first.x : first.y
      const patches = new Map<string, Partial<El>>()
      for (const { el, f } of items) {
        const d = pos - (axis === 'x' ? f.x : f.y)
        patches.set(el.id, axis === 'x' ? { x: el.x + d } : { y: el.y + d })
        pos += (axis === 'x' ? f.w : f.h) + gap
      }
      board.undo.stopCapturing()
      board.updateMany(patches)
    },
    insert: (els: El[], at?: Pt, opts: { avoidOverlap?: boolean; focus?: boolean } = {}) => {
      if (!els.length || readOnly()) return
      const b = union(els.map(frameBox))!
      const target = at ?? commands.viewportCenter()
      let dx = target.x - (b.x + b.w / 2)
      let dy = target.y - (b.y + b.h / 2)
      if (opts.avoidOverlap) {
        // Like FigJam: a template goes into free space beside what's already there.
        const content = union(board.all().filter((e) => !e.hidden).map(box))
        if (content && intersects({ x: b.x + dx, y: b.y + dy, w: b.w, h: b.h }, content)) {
          dx = content.x + content.w + 160 - b.x
          dy = content.y - b.y
        }
      }
      let z = board.topZ()
      const placed = els.map((el) => ({ ...el, x: el.x + dx, y: el.y + dy, z: z++ }))
      board.undo.stopCapturing()
      board.add(placed)
      set({ selection: placed.map((el) => el.id), tool: 'select' })
      if (opts.focus) fitBox(union(placed.map(frameBox)), 1)
    },
    viewportCenter: () => toWorld(cam(), W / 2, H / 2),
    viewport: view,
    centerOn: (p: Pt) => {
      stopFollowing()
      const z = cam().z
      setCam({ x: W / 2 - p.x * z, y: H / 2 - p.y * z, z })
    },
    reveal: (ids: string[]) => {
      const b = union(ids.map((id) => board.get(id)).filter((el): el is El => !!el).map(box))
      if (!b) return
      const v = expand(view(), -40 / cam().z)
      if (b.x >= v.x && b.y >= v.y && b.x + b.w <= v.x + v.w && b.y + b.h <= v.y + v.h) return
      stopFollowing()
      if (b.w <= v.w && b.h <= v.h) commands.centerOn(center(b))
      else fitBox(b, cam().z)
    },
    group: () => {
      const ids = st().selection.filter((id) => board.get(id))
      if (readOnly() || !ids.length) return
      board.undo.stopCapturing()
      const id = board.createGroup(ids)
      window.dispatchEvent(new CustomEvent('tratto:folder-created', { detail: id }))
    },
    ungroup: () => {
      const ids = selected().filter((el) => el.groupId).map((el) => el.id)
      if (readOnly() || !ids.length) return
      board.undo.stopCapturing()
      board.setGroup(ids, null)
    },
    follow: (id: number | null) => {
      following = id
      set({ following: id })
      applyFollow()
    },
  })

  function applyFollow() {
    if (following === null || !awareness) return
    const v = awareness.getStates().get(following)?.view
    if (!v || ![v.cx, v.cy, v.z].every((n) => typeof n === 'number' && Number.isFinite(n))) return
    const z = clamp(v.z, MIN_ZOOM, MAX_ZOOM)
    set({ camera: { x: W / 2 - v.cx * z, y: H / 2 - v.cy * z, z } })
  }

  /* ---------- wiring ---------- */

  function resize() {
    rect = root.getBoundingClientRect()
    dpr = window.devicePixelRatio || 1
    W = rect.width
    H = rect.height
    for (const c of [scene, overlay]) {
      c.width = Math.round(W * dpr)
      c.height = Math.round(H * dpr)
    }
    invalidate(true)
  }
  const ro = new ResizeObserver(resize)
  ro.observe(root)
  resize()
  // A screen with another pixel density changes devicePixelRatio without resizing anything:
  // without this the board stays blurry after moving the window there.
  let dprQuery: MediaQueryList | null = null
  const onDpr = () => {
    resize()
    watchDpr()
  }
  const watchDpr = () => {
    dprQuery?.removeEventListener('change', onDpr)
    dprQuery = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
    dprQuery.addEventListener('change', onDpr)
  }
  watchDpr()

  const camKey = `tratto.cam.${o.boardKey}`
  let camTimer = 0
  const unsubStore = useEditor.subscribe((s, prev) => {
    if (s.camera !== prev.camera) {
      invalidate(true)
      const c = s.camera
      aw('view', { cx: r2((W / 2 - c.x) / c.z), cy: r2((H / 2 - c.y) / c.z), z: Math.round(c.z * 1000) / 1000 })
      clearTimeout(camTimer)
      camTimer = window.setTimeout(() => {
        try {
          localStorage.setItem(camKey, JSON.stringify(c))
        } catch {
          /* ignore */
        }
      }, 400)
    }
    if (s.selection !== prev.selection) {
      aw('sel', s.selection.slice(0, 200))
      invalidate(false)
    }
    if (s.editingId !== prev.editingId || s.readOnly !== prev.readOnly) invalidate(true)
    if (s.prefs.accent !== prev.prefs.accent || s.prefs.bigHandles !== prev.prefs.bigHandles) invalidate(false)
    if (s.tool !== prev.tool) {
      eraserAt = null
      hoverHandle = null
      hoverId = null
      updateCursor()
      invalidate(false)
    }
  })
  const unsubBoard = board.subscribe(() => {
    if (!gesture || gesture.kind !== 'move') prune()
    invalidate(true)
  })
  const offStamps = onStampLoaded(() => invalidate(true))
  const onAwareness = () => {
    applyFollow()
    invalidate(false)
  }
  awareness?.on('change', onAwareness)

  let savedCam = false
  try {
    const c = JSON.parse(localStorage.getItem(camKey) ?? 'null')
    if (c && [c.x, c.y, c.z].every((n: unknown) => typeof n === 'number' && Number.isFinite(n))) {
      set({ camera: { x: c.x, y: c.y, z: clamp(c.z, MIN_ZOOM, MAX_ZOOM) } })
      savedCam = true
    }
  } catch {
    /* ignore */
  }
  if (!savedCam) set({ camera: { x: W / 2, y: H / 2, z: 1 } })

  const onFonts = () => {
    resetTextCaches()
    invalidate(true)
  }
  document.fonts?.addEventListener('loadingdone', onFonts)
  document.fonts?.ready.then(onFonts)

  const onDown = (e: PointerEvent) => {
    onPointerDown(e)
    startPress(e)
  }
  overlay.addEventListener('pointerdown', onDown)
  overlay.addEventListener('pointermove', onPointerMove)
  overlay.addEventListener('pointerup', onPointerUp)
  overlay.addEventListener('pointercancel', onPointerUp)
  overlay.addEventListener('pointerleave', onPointerLeave)
  overlay.addEventListener('dblclick', onDoubleClick)
  overlay.addEventListener('contextmenu', onContextMenu)
  root.addEventListener('wheel', onWheel, { passive: false })
  root.addEventListener('drop', onDrop)
  root.addEventListener('dragover', onDragOver)
  window.addEventListener('keydown', onKeyDown)
  window.addEventListener('keyup', onKeyUp)
  document.addEventListener('copy', onCopy)
  document.addEventListener('cut', onCut)
  document.addEventListener('paste', onPaste)
  const onBlur = () => {
    spaceDown = false
    updateCursor()
  }
  window.addEventListener('blur', onBlur)
  updateCursor()

  return {
    hadSavedCamera: savedCam,
    invalidate,
    destroy() {
      cancelAnimationFrame(raf)
      clearTimeout(awTimer)
      clearTimeout(camTimer)
      flushPreview()
      ro.disconnect()
      dprQuery?.removeEventListener('change', onDpr)
      unsubStore()
      unsubBoard()
      offStamps()
      awareness?.off('change', onAwareness)
      document.fonts?.removeEventListener('loadingdone', onFonts)
      overlay.removeEventListener('pointerdown', onDown)
      cancelPress()
      overlay.removeEventListener('pointermove', onPointerMove)
      overlay.removeEventListener('pointerup', onPointerUp)
      overlay.removeEventListener('pointercancel', onPointerUp)
      overlay.removeEventListener('pointerleave', onPointerLeave)
      overlay.removeEventListener('dblclick', onDoubleClick)
      overlay.removeEventListener('contextmenu', onContextMenu)
      root.removeEventListener('wheel', onWheel)
      root.removeEventListener('drop', onDrop)
      root.removeEventListener('dragover', onDragOver)
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onBlur)
      document.removeEventListener('copy', onCopy)
      document.removeEventListener('cut', onCut)
      document.removeEventListener('paste', onPaste)
    },
  }
}
