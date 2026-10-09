import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react'
import { Bold, Italic, RotateCw } from 'lucide-react'
import type { Board } from './doc.ts'
import { safeColor } from './doc.ts'
import { frameBox, toScreen } from './geometry.ts'
import { RULER_HEIGHT, RULER_LENGTH } from './ink.ts'
import { authorBand, fitText, fontCss, isDark, LINE_HEIGHT, sectionTitleBox, shapeTextColor, shapeTextLayout, STICKY_PAD, stickyLayout } from './render.ts'
import type { Awareness } from './controller.ts'
import { useEditor } from './store.ts'
import { FONT_STACK, type ShapeEl, type StickyEl, type TextEl } from './types.ts'
import { FontPicker, IconButton } from '../ui.tsx'

export function useBoardVersion(board: Board) {
  return useSyncExternalStore(board.subscribe, () => board.version)
}

/* ---------------- text editing ---------------- */

export function TextEditor({ board }: { board: Board }) {
  useBoardVersion(board)
  const editingId = useEditor((s) => s.editingId)
  const cam = useEditor((s) => s.camera)
  const el = editingId ? board.get(editingId) : undefined
  const ref = useRef<HTMLTextAreaElement & HTMLInputElement>(null)

  useEffect(() => {
    if (!editingId) return
    board.undo.stopCapturing()
    const focus = () => {
      const t = ref.current
      if (!t || document.activeElement === t) return
      t.focus({ preventScroll: true })
      t.setSelectionRange(t.value.length, t.value.length)
    }
    focus()
    const raf = requestAnimationFrame(focus) // after the click that created the box has settled
    return () => {
      cancelAnimationFrame(raf)
      board.undo.stopCapturing()
      // Deferred: in development React remounts effects once, and the box must survive that.
      setTimeout(() => {
        if (useEditor.getState().editingId === editingId) return
        const done = board.get(editingId)
        if (done?.type === 'text' && !done.text.trim()) board.remove([editingId])
      })
    }
  }, [editingId, board])

  // Stop editing if the element disappears (deleted by someone else, undone…).
  useEffect(() => {
    if (editingId && !el) useEditor.setState({ editingId: null })
  }, [editingId, el])

  if (!el || (el.type !== 'text' && el.type !== 'sticky' && el.type !== 'shape' && el.type !== 'section')) return null
  const z = cam.z
  const p = toScreen(cam, el.x, el.y)
  const finish = () => useEditor.setState({ editingId: null })
  // Focus moving to the text bar or to the font menu it opens (in a portal) keeps the box open.
  const onBlur = (e: React.FocusEvent) => {
    const to = e.relatedTarget as HTMLElement | null
    if (!to || !(e.currentTarget.contains(to) || to.closest('[data-text-tools]'))) finish()
  }
  const tools = el.type === 'section' ? null : <TextTools board={board} el={el} refocus={() => ref.current?.focus({ preventScroll: true })} />
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape' || (e.key === 'Enter' && (e.ctrlKey || e.metaKey))) {
      e.preventDefault()
      finish()
    }
    e.stopPropagation()
  }

  if (el.type === 'text') {
    return (
      <div className="text-scope" onBlur={onBlur}>
        <div className="text-edit" style={{ left: p.x, top: p.y, width: Math.max(el.w * z, 8) + el.fontSize * z, height: el.h * z, transform: `rotate(${el.rotation}rad)`, transformOrigin: `${(el.w * z) / 2}px ${(el.h * z) / 2}px` }}>
          <textarea
            ref={ref}
            aria-label="Testo"
            value={el.text}
            spellCheck={false}
            onChange={(e) => board.update(el.id, fitText(el, { text: e.target.value }))}
            onKeyDown={onKeyDown}
            style={{
              font: fontCss(el.font, el.fontSize * z, el.bold, el.italic),
              lineHeight: LINE_HEIGHT,
              color: safeColor(el.color),
              textAlign: el.align,
              whiteSpace: el.fixedWidth ? 'pre-wrap' : 'pre',
              width: el.fixedWidth ? el.w * z : '100%',
            }}
          />
        </div>
        {tools}
      </div>
    )
  }

  if (el.type === 'section') {
    // The title pill, edited in place.
    const t = sectionTitleBox(el, z)
    const a = toScreen(cam, t.x, t.y)
    return (
      <div className="text-scope" onBlur={onBlur}>
        <input
          ref={ref}
          className="section-title-edit"
          aria-label="Nome della sezione"
          value={el.name ?? ''}
          placeholder="Sezione"
          spellCheck={false}
          maxLength={80}
          onChange={(e) => board.update(el.id, { name: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              finish()
            }
            onKeyDown(e)
          }}
          style={{ left: a.x, top: a.y, height: Math.max(22, t.h * z) }}
        />
      </div>
    )
  }

  if (el.type === 'shape') {
    const shape = el as ShapeEl
    const t = shapeTextLayout(shape)
    return (
      <div className="text-scope" onBlur={onBlur}>
        <div className="text-edit shape" style={{ left: p.x, top: p.y, width: shape.w * z, height: shape.h * z, transform: `rotate(${shape.rotation}rad)` }}>
          <textarea
            ref={ref}
            aria-label="Testo della forma"
            value={shape.text ?? ''}
            spellCheck={false}
            onChange={(e) => board.update(shape.id, { text: e.target.value.slice(0, 4000) } as Partial<ShapeEl>)}
            onKeyDown={onKeyDown}
            style={{
              left: t.left * z,
              top: t.top * z,
              width: t.width * z,
              height: Math.max(t.layout.height, t.size * LINE_HEIGHT) * z,
              fontFamily: FONT_STACK[shape.font ?? 'sans'],
              fontSize: t.size * z,
              lineHeight: LINE_HEIGHT,
              color: shapeTextColor(shape),
              textAlign: 'center',
            }}
          />
        </div>
        {tools}
      </div>
    )
  }

  const sticky = el as StickyEl
  const { layout, size } = stickyLayout(sticky)
  return (
    <div className="text-scope" onBlur={onBlur}>
      <div className="text-edit sticky" style={{ left: p.x, top: p.y, width: sticky.w * z, height: sticky.h * z, padding: STICKY_PAD * z, paddingBottom: (STICKY_PAD + authorBand(sticky)) * z, transform: `rotate(${sticky.rotation}rad)` }}>
        <textarea
          ref={ref}
          aria-label="Testo della nota"
          value={sticky.text}
          spellCheck={false}
          onChange={(e) => board.update(sticky.id, { text: e.target.value.slice(0, 4000) } as Partial<StickyEl>)}
          onKeyDown={onKeyDown}
          style={{
            fontFamily: FONT_STACK[sticky.font] ?? FONT_STACK.sans,
            fontSize: size * z,
            lineHeight: LINE_HEIGHT,
            height: Math.max(layout.height, size * LINE_HEIGHT) * z,
            color: isDark(sticky.color) ? '#FFFFFF' : '#1E1E1E',
            textAlign: sticky.align,
          }}
        />
      </div>
      {tools}
    </div>
  )
}

const BAR_H = 32
const BAR_GAP = 8

/**
 * Bar over the box being edited: font, plus bold and italic for text. Its buttons don't take
 * the focus from the text (mousedown is cancelled) and the font menu gives it back on close.
 */
function TextTools({ board, el, refocus }: { board: Board; el: TextEl | StickyEl | ShapeEl; refocus: () => void }) {
  const cam = useEditor((s) => s.camera)
  const b = frameBox(el)
  const a = toScreen(cam, b.x, b.y)
  const above = a.y - BAR_GAP - BAR_H
  const top = above >= 8 ? above : toScreen(cam, b.x, b.y + b.h).y + BAR_GAP
  const width = el.type === 'text' ? 232 : 176
  const set = (patch: Partial<TextEl>) => board.update(el.id, el.type === 'text' ? fitText(el, patch) : (patch as Partial<ShapeEl>))
  const keepFocus = (e: React.MouseEvent) => e.preventDefault()
  return (
    <div
      className="text-tools"
      data-text-tools=""
      role="toolbar"
      aria-label="Formato del testo"
      style={{ left: `max(8px, min(${a.x}px, calc(100% - ${width + 8}px)))`, top }}
      // Escape on the bar leaves the text as it does in the box (not when it closes the font menu, in a portal).
      onKeyDown={(e) => e.key === 'Escape' && e.currentTarget.contains(e.target as Node) && useEditor.setState({ editingId: null })}
    >
      <FontPicker
        value={el.font ?? 'sans'}
        onChange={(font) => {
          set({ font })
          // The next text box starts with the same font.
          const prefs = useEditor.getState().prefs
          if (el.type === 'text' && prefs.text.font !== font) useEditor.getState().setPrefs({ text: { ...prefs.text, font } })
        }}
        onCloseAutoFocus={(e) => {
          e.preventDefault()
          refocus()
        }}
      />
      {el.type === 'text' && (
        <>
          <IconButton label="Grassetto" on={el.bold} onMouseDown={keepFocus} onClick={() => set({ bold: !el.bold })}>
            <Bold size={16} />
          </IconButton>
          <IconButton label="Corsivo" on={el.italic} onMouseDown={keepFocus} onClick={() => set({ italic: !el.italic })}>
            <Italic size={16} />
          </IconButton>
        </>
      )}
    </div>
  )
}

/* ---------------- other people's cursors ---------------- */

interface Peer {
  id: number
  name: string
  color: string
  x: number
  y: number
  chat: string | null
}

const HEX = /^#[0-9a-f]{6}$/i
export const peerName = (s: { user?: { name?: unknown } } | undefined) => (typeof s?.user?.name === 'string' && s.user.name.trim() ? s.user.name.trim().slice(0, 32) : 'Ospite')
export const peerColor = (s: { user?: { color?: unknown } } | undefined) => (typeof s?.user?.color === 'string' && HEX.test(s.user.color) ? s.user.color : '#9747FF')

export function usePeers(awareness: Awareness | null) {
  const [, force] = useState(0)
  useEffect(() => {
    if (!awareness) return
    let raf = 0
    const on = () => {
      if (!raf)
        raf = requestAnimationFrame(() => {
          raf = 0
          force((n) => n + 1)
        })
    }
    awareness.on('change', on)
    return () => {
      awareness.off('change', on)
      cancelAnimationFrame(raf)
    }
  }, [awareness])
  if (!awareness) return []
  return [...awareness.getStates().entries()].filter(([id]) => id !== awareness.clientID).map(([id, s]) => ({ id, state: s }))
}

export function Cursors({ awareness }: { awareness: Awareness | null }) {
  const cam = useEditor((s) => s.camera)
  const peers: Peer[] = usePeers(awareness)
    .filter(({ state }) => state.cursor && Number.isFinite(state.cursor.x) && Number.isFinite(state.cursor.y))
    .map(({ id, state }) => ({ id, name: peerName(state), color: peerColor(state), x: state.cursor.x, y: state.cursor.y, chat: typeof state.chat === 'string' && state.chat.trim() ? state.chat.slice(0, 80) : null }))
  return (
    <div className="cursors" aria-hidden="true">
      {peers.map((p) => {
        const s = toScreen(cam, p.x, p.y)
        return (
          <div key={p.id} className="cursor" style={{ transform: `translate(${s.x}px, ${s.y}px)` }}>
            <svg width="18" height="18" viewBox="0 0 18 18">
              <path d="M2 1.5 15.5 7.6 9 9.4 6.9 16Z" fill={p.color} stroke="#fff" strokeWidth="1.4" strokeLinejoin="round" />
            </svg>
            <span className="cursor-name" data-chat={p.chat ? '' : undefined} style={{ background: p.color, color: isDark(p.color) ? '#fff' : '#1E1E1E' }}>
              {p.chat ? (
                <>
                  <small>{p.name}</small>
                  {p.chat}
                </>
              ) : (
                p.name
              )}
            </span>
          </div>
        )
      })}
    </div>
  )
}

/* ---------------- ruler ---------------- */

const PX_PER_MM = 96 / 25.4

export function RulerView() {
  const r = useEditor((s) => s.ruler)
  const ref = useRef<SVGSVGElement>(null)
  // Fade in when it appears.
  useLayoutEffect(() => {
    ref.current?.animate?.([{ opacity: 0 }, { opacity: 1 }], { duration: 160, easing: 'ease-out' })
  }, [r.visible])
  if (!r.visible) return null
  const deg = ((((r.angle * 180) / Math.PI) % 360) + 360) % 360
  const shown = Math.round(deg > 180 ? 360 - deg : deg)
  const ticks: React.ReactNode[] = []
  const usable = RULER_LENGTH - 80
  const mmCount = Math.floor(usable / PX_PER_MM)
  for (let mm = 0; mm <= mmCount; mm++) {
    const x = 40 + mm * PX_PER_MM
    const len = mm % 10 === 0 ? 18 : mm % 5 === 0 ? 12 : 7
    ticks.push(<line key={mm} x1={x} x2={x} y1={0} y2={len} />)
    if (mm % 10 === 0)
      ticks.push(
        <text key={`t${mm}`} x={x} y={30} textAnchor="middle">
          {mm / 10}
        </text>,
      )
  }
  return (
    <svg
      ref={ref}
      className="ruler"
      width={RULER_LENGTH}
      height={RULER_HEIGHT}
      viewBox={`0 0 ${RULER_LENGTH} ${RULER_HEIGHT}`}
      style={{ transform: `translate(${r.x - RULER_LENGTH / 2}px, ${r.y - RULER_HEIGHT / 2}px) rotate(${r.angle}rad)` }}
      aria-label={`Righello, ${shown} gradi`}
      role="img"
    >
      <rect className="ruler-body" x="0.5" y="0.5" width={RULER_LENGTH - 1} height={RULER_HEIGHT - 1} rx="6" />
      <g className="ruler-ticks">{ticks}</g>
      <g className="ruler-ticks" transform={`translate(0 ${RULER_HEIGHT}) scale(1 -1)`}>
        {ticks.filter((t) => (t as React.ReactElement).type === 'line')}
      </g>
      <g transform={`translate(${RULER_LENGTH / 2} ${RULER_HEIGHT / 2})`}>
        <rect className="ruler-angle-bg" x="-26" y="-4" width="52" height="22" rx="11" />
        <text className="ruler-angle" y="11" textAnchor="middle">
          {shown}°
        </text>
      </g>
      <g transform={`translate(${RULER_LENGTH - 26} ${RULER_HEIGHT / 2})`} className="ruler-knob">
        <circle r="13" />
        <g transform="translate(-7 -7)">
          <RotateCw size={14} />
        </g>
      </g>
    </svg>
  )
}
