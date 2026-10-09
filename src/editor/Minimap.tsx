import { useEffect, useRef } from 'react'
import { LocateFixed } from 'lucide-react'
import type { Board } from './doc.ts'
import { commands } from './controller.ts'
import { aabb, union } from './geometry.ts'
import { useBoardVersion } from './overlays.tsx'
import { drawElement, type ImageStore } from './render.ts'
import { useEditor } from './store.ts'

const W = 176
const H = 120

/** Overview of the whole board with the visible area; click or drag to move there. */
export function Minimap({ board, images }: { board: Board; images: ImageStore }) {
  useBoardVersion(board)
  useEditor((s) => s.camera)
  const accent = useEditor((s) => s.prefs.accent)
  const ref = useRef<HTMLCanvasElement>(null)
  const frame = useRef({ x: 0, y: 0, k: 1 })
  // The drawn board, redrawn only when the board or the framing changes (not on every pan).
  const cache = useRef<{ key: string; canvas: HTMLCanvasElement } | null>(null)
  const content = union(board.all().filter((el) => !el.hidden).map(aabb))

  // Runs after every render: the board or the camera changed.
  useEffect(() => {
    const canvas = ref.current
    if (!canvas || !content) return
    const view = commands.viewport()
    const b = union([content, view])!
    const pad = 0.08 * Math.max(b.w, b.h)
    const k = Math.min(W / (b.w + pad * 2), H / (b.h + pad * 2))
    const ox = (W - b.w * k) / 2 - b.x * k
    const oy = (H - b.h * k) / 2 - b.y * k
    frame.current = { x: ox, y: oy, k }
    const dpr = window.devicePixelRatio || 1
    const key = [board.version, ox, oy, k, dpr].join('|')
    if (cache.current?.key !== key) {
      const layer = cache.current?.canvas ?? document.createElement('canvas')
      layer.width = W * dpr
      layer.height = H * dpr
      const lctx = layer.getContext('2d')!
      lctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      lctx.fillStyle = board.getMeta().background
      lctx.fillRect(0, 0, W, H)
      lctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * ox, dpr * oy)
      for (const el of board.all()) drawElement(lctx, el, { images, zoom: k })
      cache.current = { key, canvas: layer }
    }
    canvas.width = W * dpr
    canvas.height = H * dpr
    const ctx = canvas.getContext('2d')!
    ctx.drawImage(cache.current.canvas, 0, 0)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    const r = { x: view.x * k + ox, y: view.y * k + oy, w: view.w * k, h: view.h * k }
    ctx.fillStyle = accent + '1A'
    ctx.strokeStyle = accent
    ctx.lineWidth = 1.5
    ctx.fillRect(r.x, r.y, r.w, r.h)
    ctx.strokeRect(r.x, r.y, r.w, r.h)
  })

  if (!content) return null

  const go = (e: React.PointerEvent) => {
    const r = e.currentTarget.getBoundingClientRect()
    const f = frame.current
    commands.centerOn({ x: (e.clientX - r.left - f.x) / f.k, y: (e.clientY - r.top - f.y) / f.k })
  }

  return (
    <div className="minimap">
      <canvas
        ref={ref}
        style={{ width: W, height: H }}
        role="img"
        aria-label="Minimappa: clicca o trascina per spostarti sulla lavagna"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId)
          go(e)
        }}
        onPointerMove={(e) => e.buttons && go(e)}
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
