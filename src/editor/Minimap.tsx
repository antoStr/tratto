import { useEffect, useMemo, useRef } from 'react'
import { LocateFixed } from 'lucide-react'
import type { Board } from './doc.ts'
import { commands } from './controller.ts'
import { aabb, clamp, union, type Pt } from './geometry.ts'
import { useBoardVersion } from './overlays.tsx'
import { drawElement, type ImageStore } from './render.ts'
import { useEditor } from './store.ts'

const W = 176
const H = 120
/** The board picture is redrawn at most this often (ms) while it keeps changing. */
const REDRAW_MS = 250

/** Map pixel = board point * k + (x, y). */
interface Frame {
  k: number
  x: number
  y: number
}

/**
 * Overview of the whole board with the visible area; click or drag to move there.
 * It frames the content only, so the map stays still while you pan or drag in it.
 */
export function Minimap({ board, images }: { board: Board; images: ImageStore }) {
  const version = useBoardVersion(board)
  const accent = useEditor((s) => s.prefs.accent)
  const ref = useRef<HTMLCanvasElement>(null)
  const layer = useRef<HTMLCanvasElement | null>(null)
  /** Framing of the picture on screen (it lags `frame` while redraws are throttled). */
  const shown = useRef<Frame | null>(null)
  const lastDraw = useRef(-Infinity)
  /** Pointer offset from the view centre while dragging, in board units. */
  const grab = useRef<Pt | null>(null)
  // `version` changes whenever the board does.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const content = useMemo(() => union(board.all().filter((el) => !el.hidden).map(aabb)), [board, version])
  const frame = useMemo((): Frame | null => {
    if (!content) return null
    const pad = 0.08 * (Math.max(content.w, content.h) || 1)
    const k = Math.min(W / (content.w + pad * 2), H / (content.h + pad * 2))
    return { k, x: (W - content.w * k) / 2 - content.x * k, y: (H - content.h * k) / 2 - content.y * k }
  }, [content])

  useEffect(() => {
    const canvas = ref.current
    if (!canvas || !frame) return
    const dpr = window.devicePixelRatio || 1
    const l = (layer.current ??= document.createElement('canvas'))
    const drawBoard = () => {
      lastDraw.current = performance.now()
      shown.current = frame
      l.width = W * dpr
      l.height = H * dpr
      const lctx = l.getContext('2d')!
      lctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      lctx.fillStyle = board.getMeta().background
      lctx.fillRect(0, 0, W, H)
      lctx.setTransform(dpr * frame.k, 0, 0, dpr * frame.k, dpr * frame.x, dpr * frame.y)
      for (const el of board.all()) drawElement(lctx, el, { images, pixel: 1 / (frame.k * dpr), hairline: true })
    }
    // The view rectangle over the board picture: panning only repaints this.
    const paint = () => {
      if (canvas.width !== W * dpr) {
        canvas.width = W * dpr
        canvas.height = H * dpr
      }
      const f = shown.current
      if (!f) return
      const ctx = canvas.getContext('2d')!
      ctx.setTransform(1, 0, 0, 1, 0, 0)
      ctx.drawImage(l, 0, 0)
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const v = commands.viewport()
      const w = v.w * f.k
      const h = v.h * f.k
      // Far from the content the rectangle would be off the map: a sliver stays at the edge, towards it.
      const x = clamp(v.x * f.k + f.x, 3 - w, W - 3)
      const y = clamp(v.y * f.k + f.y, 3 - h, H - 3)
      ctx.fillStyle = accent + '1A'
      ctx.strokeStyle = accent
      ctx.lineWidth = 1.5
      ctx.fillRect(x, y, w, h)
      ctx.strokeRect(x, y, w, h)
    }
    paint()
    const timer = window.setTimeout(
      () => {
        drawBoard()
        paint()
      },
      Math.max(0, lastDraw.current + REDRAW_MS - performance.now()),
    )
    const unsub = useEditor.subscribe((s, prev) => {
      if (s.camera !== prev.camera) paint()
    })
    return () => {
      clearTimeout(timer)
      unsub()
    }
  }, [board, images, frame, accent])

  if (!frame) return null

  /** Board point under the pointer, kept on the map so a drag past its edge doesn't fling the view away. */
  const pointAt = (e: React.PointerEvent): Pt | null => {
    const f = shown.current
    if (!f) return null
    const r = e.currentTarget.getBoundingClientRect()
    return { x: (clamp(e.clientX - r.left, 0, W) - f.x) / f.k, y: (clamp(e.clientY - r.top, 0, H) - f.y) / f.k }
  }
  const moveTo = (p: Pt | null) => p && grab.current && commands.centerOn({ x: p.x - grab.current.x, y: p.y - grab.current.y })

  return (
    <div className="minimap">
      <canvas
        ref={ref}
        style={{ width: W, height: H }}
        role="img"
        aria-label="Minimappa: clicca o trascina per spostarti sulla lavagna"
        onPointerDown={(e) => {
          const p = pointAt(e)
          if (!p) return
          try {
            e.currentTarget.setPointerCapture(e.pointerId)
          } catch {
            /* pointer already gone */
          }
          const v = commands.viewport()
          const inside = p.x >= v.x && p.x <= v.x + v.w && p.y >= v.y && p.y <= v.y + v.h
          // Grabbing the rectangle drags it from where it was taken; elsewhere the view centres on the click.
          grab.current = inside ? { x: p.x - (v.x + v.w / 2), y: p.y - (v.y + v.h / 2) } : { x: 0, y: 0 }
          moveTo(p)
        }}
        onPointerMove={(e) => moveTo(pointAt(e))}
        onPointerUp={() => (grab.current = null)}
        onPointerCancel={() => (grab.current = null)}
      />
    </div>
  )
}

/** Shown when the content is somewhere off screen: one click brings it back. */
export function BackToContent() {
  const lost = useEditor((s) => s.lost)
  if (!lost) return null
  return (
    <button type="button" className="back-to-content" onClick={() => commands.fit()}>
      <LocateFixed size={14} aria-hidden="true" />
      Torna ai contenuti
    </button>
  )
}
