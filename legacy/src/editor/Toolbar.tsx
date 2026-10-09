import { useMemo, type ReactNode } from 'react'
import { Popover } from 'radix-ui'
import { BackToContent } from './Minimap.tsx'
import {
  ArrowBigLeft,
  ArrowBigRight,
  ArrowUpRight,
  ChevronDown,
  Circle,
  Diamond,
  Eraser,
  Hand,
  Hexagon,
  Highlighter,
  ImagePlus,
  Lasso,
  MessageCircle,
  Minus,
  MousePointer2,
  Octagon,
  Pen,
  Pentagon,
  Plus,
  Ruler,
  Shapes,
  Smile,
  Square,
  SquareDashedTopSolid,
  Star,
  Triangle,
  Type,
} from 'lucide-react'
import { FontPicker, IconButton, Menu, MenuContent, MenuItem, MenuTrigger, NumberField, Segmented, Slider, Swatches, Tip } from '../ui.tsx'
import { outlineToPath, strokeOutline } from './ink.ts'
import { useEditor, type Pen as PenPreset, type ShapeTool, type Tool } from './store.ts'
import { HIGHLIGHT_COLORS, INK_COLORS, STICKY_COLORS, TAPE_COLORS } from './types.ts'
import { STAMP_SET } from './stamps.ts'

const RoundRect = ({ size = 18 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
    <rect x="3" y="5" width="18" height="14" rx="5" />
  </svg>
)
const Parallelogram = ({ size = 18 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" aria-hidden="true">
    <path d="M7 5h15l-5 14H2Z" />
  </svg>
)

export const SHAPES: { kind: ShapeTool; label: string; icon: (size?: number) => ReactNode; kbd?: string }[] = [
  { kind: 'rect', label: 'Rettangolo', icon: (n = 18) => <Square size={n} />, kbd: 'R' },
  { kind: 'roundRect', label: 'Rettangolo arrotondato', icon: (n = 18) => <RoundRect size={n} /> },
  { kind: 'ellipse', label: 'Ellisse', icon: (n = 18) => <Circle size={n} />, kbd: 'O' },
  { kind: 'diamond', label: 'Rombo', icon: (n = 18) => <Diamond size={n} /> },
  { kind: 'triangle', label: 'Triangolo', icon: (n = 18) => <Triangle size={n} /> },
  { kind: 'triangleDown', label: 'Triangolo capovolto', icon: (n = 18) => <Triangle size={n} style={{ transform: 'rotate(180deg)' }} /> },
  { kind: 'parallelogram', label: 'Parallelogramma', icon: (n = 18) => <Parallelogram size={n} /> },
  { kind: 'pentagon', label: 'Pentagono', icon: (n = 18) => <Pentagon size={n} /> },
  { kind: 'hexagon', label: 'Esagono', icon: (n = 18) => <Hexagon size={n} /> },
  { kind: 'octagon', label: 'Ottagono', icon: (n = 18) => <Octagon size={n} /> },
  { kind: 'star', label: 'Stella', icon: (n = 18) => <Star size={n} /> },
  { kind: 'plus', label: 'Croce', icon: (n = 18) => <Plus size={n} /> },
  { kind: 'arrowRight', label: 'Freccia a destra', icon: (n = 18) => <ArrowBigRight size={n} /> },
  { kind: 'arrowLeft', label: 'Freccia a sinistra', icon: (n = 18) => <ArrowBigLeft size={n} /> },
]

const ICON = 20
const px = (v: number) => `${Math.round(v * 10) / 10} px`
const pct = (v: number) => `${Math.round(v * 100)}%`

/** Tool icon with a bar of the colour it will use, like Office's font-colour button and Whiteboard's pens. */
function Inked({ color, children }: { color: string; children: ReactNode }) {
  return (
    <span className="inked">
      {children}
      <span className="ink-bar" style={{ background: color }} />
    </span>
  )
}

/** The sticky note tool drawn as a note in the colour the next one will have (FigJam). */
function StickyGlyph({ color }: { color: string }) {
  return (
    <svg width={ICON} height={ICON} viewBox="0 0 20 20" aria-hidden="true">
      <path d="M3 3h14v10l-4 4H3Z" fill={color} stroke="currentColor" strokeOpacity="0.55" strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M17 13h-4v4" fill="none" stroke="currentColor" strokeOpacity="0.55" strokeWidth="1.2" strokeLinejoin="round" />
    </svg>
  )
}

/** Washi tape: a striped band with torn ends, in the colour the next piece will have. */
function TapeGlyph({ color }: { color: string }) {
  return (
    <svg width={ICON} height={ICON} viewBox="0 0 20 20" aria-hidden="true">
      <g transform="rotate(-30 10 10)">
        <path d="M2 7h16l-1 1.5 1 1.5-1 1.5 1 1.5H2l1-1.5-1-1.5 1-1.5Z" fill={color} stroke="currentColor" strokeOpacity="0.55" strokeWidth="1" strokeLinejoin="round" />
        <path d="M5 13l4-6M9 13l4-6M13 13l4-6" stroke="#fff" strokeOpacity="0.7" strokeWidth="1.2" />
      </g>
    </svg>
  )
}

/** Laser pointer: a red dot with its glow. */
function LaserGlyph() {
  return (
    <svg width={ICON} height={ICON} viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r="7.5" fill="#FF3B30" opacity="0.18" />
      <circle cx="10" cy="10" r="4" fill="#FF3B30" />
      <circle cx="10" cy="10" r="1.6" fill="#FFFFFF" />
    </svg>
  )
}

export function Toolbar({ onImage }: { onImage: () => void }) {
  const tool = useEditor((s) => s.tool)
  const readOnly = useEditor((s) => s.readOnly)
  const ruler = useEditor((s) => s.ruler.visible)
  const prefs = useEditor((s) => s.prefs)
  const penIndex = useEditor((s) => s.pen)
  const setTool = useEditor((s) => s.setTool)
  const pos = prefs.toolbarPos

  const btn = (t: Tool, label: string, icon: ReactNode, kbd?: string) => (
    <IconButton label={label} kbd={kbd} active={tool === t} onClick={() => setTool(t)} className="tool">
      {icon}
    </IconButton>
  )
  const shape = SHAPES.find((s) => s.kind === prefs.lastShape) ?? SHAPES[0]
  const pen = prefs.pens[penIndex] ?? prefs.pens[0]

  return (
    <div className="toolbar-wrap" data-pos={pos}>
      <BackToContent />
      {!readOnly && <Tray />}
      <div className="toolbar" role="toolbar" aria-label="Strumenti">
        {btn('select', 'Seleziona', <MousePointer2 size={ICON} />, 'V')}
        {btn('hand', 'Mano', <Hand size={ICON} />, 'H')}
        {readOnly ? (
          <>
            <span className="tb-sep" />
            {btn('laser', 'Puntatore laser', <LaserGlyph />, 'K')}
          </>
        ) : (
          <>
            <span className="tb-sep" />
            {btn(
              'pen',
              'Penna',
              <Inked color={pen.color}>
                <Pen size={ICON} />
              </Inked>,
              'P',
            )}
            {btn(
              'highlighter',
              'Evidenziatore',
              <Inked color={prefs.highlighter.color}>
                <Highlighter size={ICON} />
              </Inked>,
              'M',
            )}
            {btn('tape', 'Nastro adesivo', <TapeGlyph color={prefs.tape.color} />, 'W')}
            {btn('eraser', 'Gomma', <Eraser size={ICON} />, 'E')}
            {btn('lasso', 'Lazo', <Lasso size={ICON} />, 'Q')}
            <IconButton
              label={ruler ? 'Nascondi righello' : 'Mostra righello'}
              kbd="U"
              on={ruler}
              className="tool"
              onClick={() => {
                const s = useEditor.getState()
                s.setRuler({ visible: !ruler, ...(ruler ? {} : { x: window.innerWidth / 2, y: window.innerHeight / 2 - 60 }) })
              }}
            >
              <Ruler size={ICON} />
            </IconButton>
            <span className="tb-sep" />
            {btn('sticky', 'Nota adesiva', <StickyGlyph color={prefs.stickyColor} />, 'S')}
            <div className="tool-split">
              {btn('shape', shape.label, shape.icon(ICON), shape.kbd)}
              <Menu>
                <MenuTrigger asChild>
                  <button type="button" className="split-chevron" aria-label="Scegli forma">
                    <ChevronDown size={12} />
                  </button>
                </MenuTrigger>
                <MenuContent side={pos === 'top' ? 'bottom' : 'top'} align="center">
                  {SHAPES.map((s) => (
                    <MenuItem
                      key={s.kind}
                      icon={s.icon(14)}
                      kbd={s.kbd}
                      onSelect={() => {
                        useEditor.getState().setPrefs({ lastShape: s.kind })
                        setTool('shape')
                      }}
                    >
                      {s.label}
                    </MenuItem>
                  ))}
                </MenuContent>
              </Menu>
            </div>
            {btn('arrow', 'Connettore e freccia: parti da una forma per collegarla', <ArrowUpRight size={ICON} />, 'X')}
            {btn('line', 'Linea', <Minus size={ICON} />, 'L')}
            {btn('text', 'Testo', <Type size={ICON} />, 'T')}
            {btn('section', 'Sezione', <SquareDashedTopSolid size={ICON} />, 'Maiusc+S')}
            <span className="tb-sep" />
            {btn('stamp', 'Reazioni', <Smile size={ICON} />)}
            {btn('comment', 'Commento', <MessageCircle size={ICON} />, 'C')}
            <IconButton label="Inserisci immagine" kbd="I" className="tool" onClick={onImage}>
              <ImagePlus size={ICON} />
            </IconButton>
            <span className="tb-sep" />
            {btn('laser', 'Puntatore laser', <LaserGlyph />, 'K')}
          </>
        )}
      </div>
    </div>
  )
}

/* ---------- contextual tray above the toolbar ---------- */

function Tray() {
  const tool = useEditor((s) => s.tool)
  const prefs = useEditor((s) => s.prefs)
  const penIndex = useEditor((s) => s.pen)
  const setPrefs = useEditor((s) => s.setPrefs)
  const pen = prefs.pens[penIndex] ?? prefs.pens[0]
  const setPen = (patch: Partial<PenPreset>) => setPrefs({ pens: prefs.pens.map((p, i) => (i === penIndex ? { ...p, ...patch } : p)) })
  const side = prefs.toolbarPos === 'top' ? 'bottom' : 'top'

  let content: ReactNode = null
  switch (tool) {
    case 'pen':
      content = (
        <>
          <div className="pen-set" role="group" aria-label="Penne">
            {prefs.pens.map((p, i) => (
              <PenSlot key={i} index={i} pen={p} active={i === penIndex} side={side} />
            ))}
          </div>
          <span className="tb-sep" />
          <Slider className="tray-slider" label="Spessore" value={pen.size} min={1} max={64} step={0.5} log format={px} onChange={(size) => setPen({ size })} />
          <Slider className="tray-slider" label="Opacità" value={pen.opacity ?? 1} min={0.1} max={1} step={0.05} format={pct} onChange={(opacity) => setPen({ opacity })} />
          <span className="tb-sep" />
          <IconButton label="Da tratto a forma: trasforma linee, cerchi e rettangoli disegnati a mano" on={prefs.inkToShape} onClick={() => setPrefs({ inkToShape: !prefs.inkToShape })}>
            <Shapes size={18} />
          </IconButton>
          <IconButton label={prefs.fingerDraw ? 'Le dita disegnano (tocca per usarle per spostarti)' : 'Le dita spostano la lavagna (tocca per disegnare con le dita)'} on={prefs.fingerDraw} onClick={() => setPrefs({ fingerDraw: !prefs.fingerDraw })}>
            <Hand size={18} />
          </IconButton>
        </>
      )
      break
    case 'highlighter':
      content = (
        <>
          <Swatches label="Colore evidenziatore" colors={HIGHLIGHT_COLORS} value={prefs.highlighter.color} onChange={(color) => setPrefs({ highlighter: { ...prefs.highlighter, color } })} />
          <span className="tb-sep" />
          <Slider className="tray-slider" label="Spessore" value={prefs.highlighter.size} min={4} max={96} log format={px} onChange={(size) => setPrefs({ highlighter: { ...prefs.highlighter, size } })} />
        </>
      )
      break
    case 'tape':
      content = (
        <>
          <Swatches label="Colore del nastro" colors={TAPE_COLORS} value={prefs.tape.color} onChange={(color) => setPrefs({ tape: { ...prefs.tape, color } })} />
          <span className="tb-sep" />
          <Slider className="tray-slider" label="Larghezza" value={prefs.tape.size} min={8} max={120} log format={px} onChange={(size) => setPrefs({ tape: { ...prefs.tape, size } })} />
        </>
      )
      break
    case 'comment':
      content = <span className="tray-hint">Clicca dove vuoi lasciare un commento. Gli altri lo vedono e possono rispondere.</span>
      break
    case 'eraser':
      content = (
        <>
          <div style={{ width: 196 }}>
            <Segmented
              label="Modalità gomma"
              value={prefs.eraser.mode}
              onChange={(mode) => setPrefs({ eraser: { ...prefs.eraser, mode } })}
              options={[
                { value: 'pixel', label: 'Pixel', title: 'Pixel: cancella solo dove passi, come Paint' },
                { value: 'stroke', label: 'Tratto intero', title: 'Tratto intero: toglie ogni tratto o forma che tocchi' },
              ]}
            />
          </div>
          <span className="tb-sep" />
          <EraserDot size={prefs.eraser.size} />
          <Slider className="tray-slider" label="Dimensione" value={prefs.eraser.size} min={2} max={240} log format={px} onChange={(size) => setPrefs({ eraser: { ...prefs.eraser, size } })} />
          {prefs.eraser.mode === 'pixel' && <Slider className="tray-slider" label="Forza" value={prefs.eraser.strength} min={0.1} max={1} step={0.05} format={pct} onChange={(strength) => setPrefs({ eraser: { ...prefs.eraser, strength } })} />}
        </>
      )
      break
    case 'shape':
    case 'line':
    case 'arrow':
      content = (
        <>
          {tool === 'shape' && (
            <div className="shape-grid" role="radiogroup" aria-label="Forma">
              {SHAPES.map((s) => (
                <Tip key={s.kind} label={s.label} kbd={s.kbd}>
                  <button type="button" role="radio" aria-checked={prefs.lastShape === s.kind} aria-label={s.label} className="shape-btn" onClick={() => setPrefs({ lastShape: s.kind })}>
                    {s.icon(18)}
                  </button>
                </Tip>
              ))}
            </div>
          )}
          {tool === 'shape' && <span className="tb-sep" />}
          {tool === 'shape' && <ColorPop label="Riempimento" value={prefs.shapeStyle.fill} transparent side={side} onChange={(fill) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, fill } })} />}
          <ColorPop label="Contorno" value={prefs.shapeStyle.stroke} ring side={side} onChange={(stroke) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, stroke } })} />
          <span className="tb-sep" />
          <Slider className="tray-slider" label="Spessore" value={prefs.shapeStyle.strokeWidth} min={0.5} max={32} step={0.5} log format={px} onChange={(strokeWidth) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, strokeWidth } })} />
        </>
      )
      break
    case 'text':
      content = (
        <>
          <div style={{ width: 168 }}>
            <FontPicker value={prefs.text.font} onChange={(font) => setPrefs({ text: { ...prefs.text, font } })} />
          </div>
          <span className="tb-sep" />
          <ColorPop label="Colore testo" value={prefs.text.color} side={side} onChange={(color) => setPrefs({ text: { ...prefs.text, color } })} />
          <div style={{ width: 150 }}>
            <Segmented
              label="Dimensione testo"
              value={String(prefs.text.fontSize)}
              onChange={(v) => setPrefs({ text: { ...prefs.text, fontSize: Number(v) } })}
              options={[
                { value: '16', label: 'S', title: 'Piccolo' },
                { value: '24', label: 'M', title: 'Medio' },
                { value: '36', label: 'L', title: 'Grande' },
                { value: '56', label: 'XL', title: 'Titolo' },
              ]}
            />
          </div>
          <div style={{ width: 72 }}>
            <NumberField label="Aa" title="Dimensione del testo in pixel" min={4} max={400} value={prefs.text.fontSize} onChange={(fontSize) => setPrefs({ text: { ...prefs.text, fontSize } })} />
          </div>
        </>
      )
      break
    case 'sticky':
      content = <Swatches label="Colore nota" colors={STICKY_COLORS} value={prefs.stickyColor} onChange={(stickyColor) => setPrefs({ stickyColor })} />
      break
    case 'section':
      content = <span className="tray-hint">Trascina per disegnare una sezione: quello che ci metti dentro si sposta insieme a lei.</span>
      break
    case 'stamp':
      content = <StampPicker value={prefs.stamp} onChange={(stamp) => setPrefs({ stamp })} />
      break
  }
  if (!content) return null
  return (
    <div className="tray" key={tool}>
      {content}
    </div>
  )
}

/** The eraser's size, drawn to scale up to the tray's height. */
function EraserDot({ size }: { size: number }) {
  const d = Math.max(4, Math.min(28, size))
  return (
    <span className="eraser-dot" aria-hidden="true">
      <span style={{ width: d, height: d }} />
    </span>
  )
}

/** Grid of reactions, drawn with the same pictures used on the board. */
export function StampPicker({ value, onChange }: { value: string | null; onChange: (emoji: string) => void }) {
  return (
    <div className="stamps" role="radiogroup" aria-label="Reazione">
      {STAMP_SET.map((s) => (
        <Tip key={s.emoji} label={s.name}>
          <button type="button" role="radio" aria-checked={value === s.emoji} aria-label={s.name} className="stamp-btn" onClick={() => onChange(s.emoji)}>
            <img src={s.url} alt="" width={22} height={22} draggable={false} />
          </button>
        </Tip>
      ))}
    </div>
  )
}

/** A pen in the tray, drawn as a real stroke in its colour, thickness and opacity. */
function PenSlot({ index, pen, active, side }: { index: number; pen: PenPreset; active: boolean; side: 'top' | 'bottom' }) {
  const path = useMemo(() => {
    const pts: number[] = []
    for (let i = 0; i <= 24; i++) {
      const t = i / 24
      pts.push(4 + t * 40, 14 + Math.sin(t * Math.PI * 2) * 6, 0.35 + Math.sin(t * Math.PI) * 0.55)
    }
    return outlineToPath(strokeOutline(pts, Math.min(pen.size * 1.2, 12), false))
  }, [pen.size])
  const setPrefs = useEditor((s) => s.setPrefs)
  const prefs = useEditor((s) => s.prefs)
  const update = (patch: Partial<PenPreset>) => setPrefs({ pens: prefs.pens.map((p, i) => (i === index ? { ...p, ...patch } : p)) })
  const svg = (
    <svg width="48" height="28" viewBox="0 0 48 28" aria-hidden="true">
      <path d={path} fill={pen.color} fillOpacity={pen.opacity ?? 1} stroke={pen.color.toUpperCase() === '#FFFFFF' ? 'rgba(0,0,0,.35)' : 'none'} strokeWidth="0.75" />
    </svg>
  )
  if (!active)
    return (
      <Tip label={`Penna ${index + 1}`} kbd={String(index + 1)}>
        <button type="button" aria-pressed={false} aria-label={`Penna ${index + 1}`} className="pen-slot" onClick={() => useEditor.setState({ pen: index })}>
          {svg}
        </button>
      </Tip>
    )
  return (
    <Popover.Root>
      <Tip label="Colore della penna" kbd={String(index + 1)}>
        <Popover.Trigger asChild>
          <button type="button" aria-pressed={true} aria-label={`Penna ${index + 1}, tocca per cambiarne il colore`} className="pen-slot active">
            {svg}
          </button>
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content className="popover pen-pop" side={side} sideOffset={12} collisionPadding={12}>
          <div className="pop-title">Colore</div>
          <Swatches label="Colore penna" colors={INK_COLORS} value={pen.color} onChange={(color) => update({ color })} />
          <div className="pop-title">Tratto</div>
          <Slider label="Spessore" value={pen.size} min={1} max={64} step={0.5} log format={px} onChange={(size) => update({ size })} />
          <Slider label="Opacità" value={pen.opacity ?? 1} min={0.1} max={1} step={0.05} format={pct} onChange={(opacity) => update({ opacity })} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

export function ColorPop({ label, value, onChange, transparent, ring, side = 'top' }: { label: string; value: string; onChange: (c: string) => void; transparent?: boolean; ring?: boolean; side?: 'top' | 'bottom' | 'left' }) {
  return (
    <Popover.Root>
      <Tip label={label}>
        <Popover.Trigger asChild>
          <button type="button" className={`color-chip ${ring ? 'ring' : ''} ${value === 'transparent' ? 'none' : ''}`} aria-label={`${label}: ${value === 'transparent' ? 'nessuno' : value}`} style={{ ['--chip' as string]: value === 'transparent' ? 'transparent' : value }} />
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content className="popover" side={side} sideOffset={12} collisionPadding={12} style={{ width: 232 }}>
          <div className="pop-title">{label}</div>
          <Swatches label={label} colors={INK_COLORS} value={value} allowTransparent={transparent} onChange={onChange} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}
