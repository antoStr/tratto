import { useEffect, useMemo, useRef } from 'react'
import { LocateFixed, Map as MapIcon, Minus, Plus, X } from 'lucide-react'
import type { Board } from './doc.ts'
import { commands } from './controller.ts'
import { aabb, clamp, union, type Pt } from './geometry.ts'
import { useBoardVersion } from './overlays.tsx'
import { drawElement, type ImageStore } from './render.ts'
import { toggleMinimap, useEditor } from './store.ts'
import { IconButton, Menu, MenuContent, MenuItem, MenuSep, MenuTrigger } from '../ui.tsx'

const W = 192
const H = 128
/** The board picture is redrawn at most this often (ms) while it keeps changing. */
const REDRAW_MS = 250

/** Map pixel = board point * k + (x, y). */
interface Frame {
  k: number
  x: number
  y: number
}

/** Framing that shows `b` whole, centred, with a margin. */
function frameFor(b: { x: number; y: number; w: number; h: number }): Frame {
  const pad = 0.06 * (Math.max(b.w, b.h) || 1)
  const k = Math.min(W / (b.w + pad * 2), H / (b.h + pad * 2))
  return { k, x: (W - b.w * k) / 2 - b.x * k, y: (H - b.h * k) / 2 - b.y * k }
}

/**
 * Overview of the board and of the visible area; click or drag to move there. It frames the
 * content and the view together, so the view rectangle is always whole on the map. While you
 * drag in it the framing stays still.
 */
export function Minimap({ board, images }: { board: Board; images: ImageStore }) {
  const version = useBoardVersion(board)
  const accent = useEditor((s) => s.prefs.accent)
  const ref = useRef<HTMLCanvasElement>(null)
  const layer = useRef<HTMLCanvasElement | null>(null)
  /** Framing of the picture on screen. */
  const shown = useRef<Frame | null>(null)
  const lastDraw = useRef(-Infinity)
  /** Pointer offset from the view centre while dragging, in board units. */
  const grab = useRef<Pt | null>(null)
  // `version` changes whenever the board does.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const content = useMemo(() => union(board.all().filter((el) => !el.hidden).map(aabb)), [board, version])

  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    const dpr = window.devicePixelRatio || 1
    const l = (layer.current ??= document.createElement('canvas'))
    const target = () => frameFor(union(content ? [content, commands.viewport()] : [commands.viewport()])!)
    const drawBoard = () => {
      lastDraw.current = performance.now()
      const frame = target()
      shown.current = frame
      l.width = W * dpr
      l.height = H * dpr
      const lctx = l.getContext('2d')!
      lctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      lctx.fillStyle = board.getMeta().background
      lctx.fillRect(0, 0, W, H)
      lctx.setTransform(dpr * frame.k, 0, 0, dpr * frame.k, dpr * frame.x, dpr * frame.y)
      for (const el of board.paintOrder()) drawElement(lctx, el, { images, pixel: 1 / (frame.k * dpr), hairline: true })
    }
    let timer = 0
    const redrawSoon = () => {
      if (timer) return
      timer = window.setTimeout(
        () => {
          timer = 0
          drawBoard()
          paint()
        },
        Math.max(0, lastDraw.current + REDRAW_MS - performance.now()),
      )
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
      const x = v.x * f.k + f.x
      const y = v.y * f.k + f.y
      // Dim what is outside the view, like Miro: the bright window is where you are.
      ctx.fillStyle = 'rgba(0,0,0,0.06)'
      ctx.beginPath()
      ctx.rect(0, 0, W, H)
      ctx.rect(x, y, w, h)
      ctx.fill('evenodd')
      ctx.strokeStyle = accent
      ctx.lineWidth = 1.5
      ctx.strokeRect(clamp(x, 0.75, W - 0.75), clamp(y, 0.75, H - 0.75), Math.min(w, W - 1.5), Math.min(h, H - 1.5))
      // Out of frame (panned or zoomed away) or much smaller than it could be: reframe, but never mid-drag.
      if (!grab.current) {
        const t = target()
        if (Math.abs(Math.log(t.k / f.k)) > 0.05 || Math.hypot(t.x - f.x, t.y - f.y) > 2) redrawSoon()
      }
    }
    paint()
    redrawSoon()
    const unsub = useEditor.subscribe((s, prev) => {
      if (s.camera !== prev.camera) paint()
    })
    return () => {
      clearTimeout(timer)
      unsub()
    }
  }, [board, images, content, accent])

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
          const inView = p.x >= v.x && p.x <= v.x + v.w && p.y >= v.y && p.y <= v.y + v.h
          // Grabbing the rectangle drags it from where it was taken; elsewhere the view centres on the click.
          grab.current = inView ? { x: p.x - (v.x + v.w / 2), y: p.y - (v.y + v.h / 2) } : { x: 0, y: 0 }
          moveTo(p)
        }}
        onPointerMove={(e) => moveTo(pointAt(e))}
        onPointerUp={() => (grab.current = null)}
        onPointerCancel={() => (grab.current = null)}
      />
      <IconButton label="Chiudi la minimappa" className="minimap-close" tipSide="left" onClick={toggleMinimap}>
        <X size={14} />
      </IconButton>
    </div>
  )
}

/** Bottom right, Whiteboard style: minimap on/off, zoom in, zoom level (menu), zoom out. */
export function ViewControls() {
  const z = useEditor((s) => s.camera.z)
  const minimap = useEditor((s) => s.prefs.minimap)
  return (
    <div className="view-controls" role="toolbar" aria-label="Vista" aria-orientation="vertical">
      <IconButton label={minimap ? 'Nascondi la minimappa' : 'Mostra la minimappa'} on={minimap} tipSide="left" onClick={toggleMinimap}>
        <MapIcon size={16} />
      </IconButton>
      <span className="vc-sep" />
      <IconButton label="Ingrandisci" kbd="Ctrl++" tipSide="left" onClick={() => commands.zoomBy(1.25)}>
        <Plus size={16} />
      </IconButton>
      <Menu>
        <MenuTrigger asChild>
          <button type="button" className="vc-zoom num" aria-label={`Zoom ${Math.round(z * 100)}%, apri il menu dello zoom`}>
            {Math.round(z * 100)}%
          </button>
        </MenuTrigger>
        <ZoomItems side="left" />
      </Menu>
      <IconButton label="Riduci" kbd="Ctrl+−" tipSide="left" onClick={() => commands.zoomBy(0.8)}>
        <Minus size={16} />
      </IconButton>
    </div>
  )
}

/** Zoom menu entries, shared by the Design panel and the view controls. */
export function ZoomItems({ side = 'bottom' }: { side?: 'bottom' | 'left' }) {
  const minimap = useEditor((s) => s.prefs.minimap)
  return (
    <MenuContent align="end" side={side}>
      <MenuItem kbd="Ctrl++" onSelect={() => commands.zoomBy(1.25)}>
        Ingrandisci
      </MenuItem>
      <MenuItem kbd="Ctrl+−" onSelect={() => commands.zoomBy(0.8)}>
        Riduci
      </MenuItem>
      <MenuItem kbd="Maiusc+1" onSelect={() => commands.fit()}>
        Adatta alla lavagna
      </MenuItem>
      <MenuItem kbd="Maiusc+2" onSelect={() => commands.fitSelection()}>
        Adatta alla selezione
      </MenuItem>
      <MenuSep />
      <MenuItem onSelect={() => commands.zoomTo(0.5)}>50%</MenuItem>
      <MenuItem kbd="Maiusc+0" onSelect={() => commands.zoomTo(1)}>
        100%
      </MenuItem>
      <MenuItem onSelect={() => commands.zoomTo(2)}>200%</MenuItem>
      <MenuSep />
      <MenuItem icon={<MapIcon size={14} />} onSelect={toggleMinimap}>
        {minimap ? 'Nascondi la minimappa' : 'Mostra la minimappa'}
      </MenuItem>
    </MenuContent>
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
