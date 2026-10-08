import { useMemo, type ReactNode } from 'react'
import { Popover } from 'radix-ui'
import {
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
  Minus,
  MousePointer2,
  Pen,
  Pointer,
  Ruler,
  Shapes,
  Smile,
  Square,
  Star,
  StickyNote,
  Triangle,
  Type,
} from 'lucide-react'
import { IconButton, Menu, MenuContent, MenuItem, MenuTrigger, Segmented, Swatches, Tip } from '../ui.tsx'
import { outlineToPath, strokeOutline } from './ink.ts'
import { ERASER_SIZES, HIGHLIGHTER_SIZES, PEN_SIZES, useEditor, type Pen as PenPreset, type Tool } from './store.ts'
import { FONT_STACK, HIGHLIGHT_COLORS, INK_COLORS, STAMPS, STICKY_COLORS, type FontKind, type ShapeKind } from './types.ts'

const SHAPES: { kind: ShapeKind; label: string; icon: ReactNode; kbd?: string }[] = [
  { kind: 'rect', label: 'Rettangolo', icon: <Square size={18} />, kbd: 'R' },
  { kind: 'ellipse', label: 'Ellisse', icon: <Circle size={18} />, kbd: 'O' },
  { kind: 'triangle', label: 'Triangolo', icon: <Triangle size={18} /> },
  { kind: 'diamond', label: 'Rombo', icon: <Diamond size={18} /> },
  { kind: 'hexagon', label: 'Esagono', icon: <Hexagon size={18} /> },
  { kind: 'star', label: 'Stella', icon: <Star size={18} /> },
]

const ICON = 20

export function Toolbar({ onImage }: { onImage: () => void }) {
  const tool = useEditor((s) => s.tool)
  const readOnly = useEditor((s) => s.readOnly)
  const ruler = useEditor((s) => s.ruler.visible)
  const lastShape = useEditor((s) => s.prefs.lastShape)
  const setTool = useEditor((s) => s.setTool)

  const btn = (t: Tool, label: string, icon: ReactNode, kbd?: string) => (
    <IconButton label={label} kbd={kbd} active={tool === t} onClick={() => setTool(t)} className="tool">
      {icon}
    </IconButton>
  )
  const shape = SHAPES.find((s) => s.kind === lastShape) ?? SHAPES[0]

  return (
    <div className="toolbar-wrap">
      {!readOnly && <Tray />}
      <div className="toolbar" role="toolbar" aria-label="Strumenti">
        {btn('select', 'Seleziona', <MousePointer2 size={ICON} />, 'V')}
        {btn('hand', 'Mano', <Hand size={ICON} />, 'H')}
        {readOnly ? (
          <>
            <span className="tb-sep" />
            {btn('laser', 'Puntatore laser', <Pointer size={ICON} />, 'K')}
          </>
        ) : (
          <>
            <span className="tb-sep" />
            {btn('pen', 'Penna', <Pen size={ICON} />, 'P')}
            {btn('highlighter', 'Evidenziatore', <Highlighter size={ICON} />, 'M')}
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
            <div className="tool-split">
              {btn('shape', shape.label, shape.icon, shape.kbd)}
              <Menu>
                <MenuTrigger asChild>
                  <button type="button" className="split-chevron" aria-label="Scegli forma">
                    <ChevronDown size={12} />
                  </button>
                </MenuTrigger>
                <MenuContent side="top" align="center">
                  {SHAPES.map((s) => (
                    <MenuItem
                      key={s.kind}
                      icon={s.icon}
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
            {btn('arrow', 'Freccia', <ArrowUpRight size={ICON} />, 'Maiusc+L')}
            {btn('line', 'Linea', <Minus size={ICON} />, 'L')}
            {btn('text', 'Testo', <Type size={ICON} />, 'T')}
            {btn('sticky', 'Nota adesiva', <StickyNote size={ICON} />, 'S')}
            {btn('stamp', 'Reazioni', <Smile size={ICON} />)}
            <IconButton label="Inserisci immagine" kbd="I" className="tool" onClick={onImage}>
              <ImagePlus size={ICON} />
            </IconButton>
            <span className="tb-sep" />
            {btn('laser', 'Puntatore laser', <Pointer size={ICON} />, 'K')}
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
  const pen = useEditor((s) => s.pen)
  const setPrefs = useEditor((s) => s.setPrefs)

  let content: ReactNode = null
  switch (tool) {
    case 'pen':
      content = (
        <>
          <div className="pen-set" role="radiogroup" aria-label="Penne">
            {prefs.pens.map((p, i) => (
              <PenSlot key={i} index={i} pen={p} active={i === pen} />
            ))}
          </div>
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
          <Sizes sizes={HIGHLIGHTER_SIZES} value={prefs.highlighter.size} onChange={(size) => setPrefs({ highlighter: { ...prefs.highlighter, size } })} color={prefs.highlighter.color} />
        </>
      )
      break
    case 'eraser':
      content = (
        <>
          <div style={{ width: 220 }}>
            <Segmented
              label="Modalità gomma"
              value={prefs.eraser.mode}
              onChange={(mode) => setPrefs({ eraser: { ...prefs.eraser, mode } })}
              options={[
                { value: 'stroke', label: 'Tratto intero' },
                { value: 'precise', label: 'Precisione' },
              ]}
            />
          </div>
          <span className="tb-sep" />
          <Sizes sizes={ERASER_SIZES} value={prefs.eraser.size} onChange={(size) => setPrefs({ eraser: { ...prefs.eraser, size } })} scale={0.4} />
        </>
      )
      break
    case 'shape':
    case 'line':
    case 'arrow':
      content = (
        <>
          {tool === 'shape' &&
            SHAPES.map((s) => (
              <IconButton key={s.kind} label={s.label} on={prefs.lastShape === s.kind} onClick={() => setPrefs({ lastShape: s.kind })}>
                {s.icon}
              </IconButton>
            ))}
          {tool === 'shape' && <span className="tb-sep" />}
          {tool === 'shape' && <ColorPop label="Riempimento" value={prefs.shapeStyle.fill} transparent onChange={(fill) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, fill } })} />}
          <ColorPop label="Contorno" value={prefs.shapeStyle.stroke} ring onChange={(stroke) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, stroke } })} />
          <span className="tb-sep" />
          <Sizes sizes={[1, 3, 6]} value={prefs.shapeStyle.strokeWidth} onChange={(strokeWidth) => setPrefs({ shapeStyle: { ...prefs.shapeStyle, strokeWidth } })} color={prefs.shapeStyle.stroke} />
        </>
      )
      break
    case 'text':
      content = (
        <>
          <div style={{ width: 260 }}>
            <Segmented
              label="Carattere"
              value={prefs.text.font}
              onChange={(font: FontKind) => setPrefs({ text: { ...prefs.text, font } })}
              options={[
                { value: 'sans', label: <span style={{ fontFamily: FONT_STACK.sans }}>Sans</span> },
                { value: 'serif', label: <span style={{ fontFamily: FONT_STACK.serif }}>Serif</span> },
                { value: 'mono', label: <span style={{ fontFamily: FONT_STACK.mono }}>Mono</span> },
                { value: 'hand', label: <span style={{ fontFamily: FONT_STACK.hand, fontSize: 14 }}>Mano</span> },
              ]}
            />
          </div>
          <span className="tb-sep" />
          <ColorPop label="Colore testo" value={prefs.text.color} onChange={(color) => setPrefs({ text: { ...prefs.text, color } })} />
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
        </>
      )
      break
    case 'sticky':
      content = <Swatches label="Colore nota" colors={STICKY_COLORS} value={prefs.stickyColor} onChange={(stickyColor) => setPrefs({ stickyColor })} />
      break
    case 'stamp':
      content = (
        <div className="stamps" role="radiogroup" aria-label="Reazione">
          {STAMPS.map((s) => (
            <button key={s} type="button" role="radio" aria-checked={prefs.stamp === s} className="stamp-btn" onClick={() => setPrefs({ stamp: s })}>
              {s}
            </button>
          ))}
        </div>
      )
      break
  }
  if (!content) return null
  return (
    <div className="tray" key={tool}>
      {content}
    </div>
  )
}

/** A pen in the tray, drawn as a real stroke in its colour and thickness. */
function PenSlot({ index, pen, active }: { index: number; pen: PenPreset; active: boolean }) {
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
      <path d={path} fill={pen.color} stroke={pen.color.toUpperCase() === '#FFFFFF' ? 'rgba(0,0,0,.35)' : 'none'} strokeWidth="0.75" />
    </svg>
  )
  if (!active)
    return (
      <Tip label={`Penna ${index + 1}`} kbd={String(index + 1)}>
        <button type="button" role="radio" aria-checked={false} aria-label={`Penna ${index + 1}`} className="pen-slot" onClick={() => useEditor.setState({ pen: index })}>
          {svg}
        </button>
      </Tip>
    )
  return (
    <Popover.Root>
      <Tip label="Modifica penna" kbd={String(index + 1)}>
        <Popover.Trigger asChild>
          <button type="button" role="radio" aria-checked={true} aria-label={`Penna ${index + 1}, tocca per modificarla`} className="pen-slot active">
            {svg}
          </button>
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content className="popover pen-pop" side="top" sideOffset={12} collisionPadding={12}>
          <div className="pop-title">Colore</div>
          <Swatches label="Colore penna" colors={INK_COLORS} value={pen.color} onChange={(color) => update({ color })} />
          <div className="pop-title">Spessore</div>
          <Sizes sizes={PEN_SIZES} value={pen.size} onChange={(size) => update({ size })} color={pen.color} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

function Sizes({ sizes, value, onChange, color = 'currentColor', scale = 1 }: { sizes: number[]; value: number; onChange: (n: number) => void; color?: string; scale?: number }) {
  return (
    <div className="sizes" role="radiogroup" aria-label="Spessore">
      {sizes.map((s) => (
        <button key={s} type="button" role="radio" aria-checked={s === value} aria-label={`${s} pixel`} title={`${s} px`} className="size-btn" onClick={() => onChange(s)}>
          <span style={{ width: Math.max(4, Math.min(22, s * scale + 2)), height: Math.max(4, Math.min(22, s * scale + 2)), background: color === '#FFFFFF' ? '#d4d4d4' : color }} />
        </button>
      ))}
    </div>
  )
}

export function ColorPop({ label, value, onChange, transparent, ring }: { label: string; value: string; onChange: (c: string) => void; transparent?: boolean; ring?: boolean }) {
  return (
    <Popover.Root>
      <Tip label={label}>
        <Popover.Trigger asChild>
          <button type="button" className={`color-chip ${ring ? 'ring' : ''} ${value === 'transparent' ? 'none' : ''}`} aria-label={`${label}: ${value === 'transparent' ? 'nessuno' : value}`} style={{ ['--chip' as string]: value === 'transparent' ? 'transparent' : value }} />
        </Popover.Trigger>
      </Tip>
      <Popover.Portal>
        <Popover.Content className="popover" side="top" sideOffset={12} collisionPadding={12} style={{ width: 232 }}>
          <div className="pop-title">{label}</div>
          <Swatches label={label} colors={INK_COLORS} value={value} allowTransparent={transparent} onChange={onChange} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}
