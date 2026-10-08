import type { ReactNode } from 'react'
import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalDistributeCenter,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalDistributeCenter,
  ArrowDownToLine,
  ArrowUpToLine,
  Bold,
  ChevronDown,
  CopyPlus,
  Eye,
  EyeOff,
  Italic,
  Lock,
  MoveLeft,
  MoveRight,
  RotateCw,
  Share2,
  TextAlignCenter,
  TextAlignEnd,
  TextAlignStart,
  Trash2,
  Unlock,
} from 'lucide-react'
import type { Board } from './doc.ts'
import { commands, type Awareness } from './controller.ts'
import { center, frameBox, scaleElement, union } from './geometry.ts'
import { fontCss, layoutText } from './render.ts'
import { peerColor, peerName, useBoardVersion, usePeers } from './overlays.tsx'
import { useEditor } from './store.ts'
import { BACKGROUNDS, FONT_STACK, INK_COLORS, STAMPS, STICKY_COLORS, type Align, type El, type FontKind, type Pattern, type ShapeKind, type TextEl } from './types.ts'
import { Avatar, IconButton, Menu, MenuContent, MenuItem, MenuSep, MenuTrigger, NumberField, Segmented, Swatches, Tip } from '../ui.tsx'

const TYPE_LABEL: Record<El['type'], string> = {
  ink: 'Tratto',
  highlighter: 'Evidenziatura',
  shape: 'Forma',
  line: 'Linea',
  text: 'Testo',
  sticky: 'Nota adesiva',
  image: 'Immagine',
  stamp: 'Reazione',
}

export function elementLabel(el: El) {
  if (el.name) return el.name
  if (el.type === 'text' || el.type === 'sticky') return el.text.trim().split('\n')[0].slice(0, 40) || TYPE_LABEL[el.type]
  if (el.type === 'line') return el.arrowEnd || el.arrowStart ? 'Freccia' : 'Linea'
  if (el.type === 'shape') return { rect: 'Rettangolo', ellipse: 'Ellisse', triangle: 'Triangolo', diamond: 'Rombo', star: 'Stella', hexagon: 'Esagono', polygon: 'Poligono' }[el.shape]
  if (el.type === 'stamp') return `Reazione ${el.emoji}`
  return TYPE_LABEL[el.type]
}

/** Main colour of an element, edited by the shared "Colore" control of a mixed selection. */
function colorKey(el: El): 'color' | 'stroke' | null {
  if (el.type === 'ink' || el.type === 'highlighter' || el.type === 'text' || el.type === 'sticky') return 'color'
  if (el.type === 'shape' || el.type === 'line') return 'stroke'
  return null
}

function Section({ title, children, actions }: { title?: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <section className="insp-section">
      {title && (
        <div className="insp-title">
          <h3>{title}</h3>
          {actions}
        </div>
      )}
      {children}
    </section>
  )
}

interface PanelProps {
  board: Board
  awareness: Awareness | null
  host: boolean
  access: 'edit' | 'view'
  following: number | null
  onShare: () => void
  onExport: () => void
  sharing: boolean
}

export function RightPanel(p: PanelProps) {
  useBoardVersion(p.board)
  const selection = useEditor((s) => s.selection)
  const els = selection.map((id) => p.board.get(id)).filter((e): e is El => !!e)
  return (
    <aside className="panel panel-right" aria-label="Proprietà">
      <div className="panel-head">
        <Participants awareness={p.awareness} following={p.following} />
        <div className="grow" />
        {p.host ? (
          <button type="button" className={`btn ${p.sharing ? 'btn-secondary' : 'btn-primary'} share-btn`} onClick={p.onShare}>
            <Share2 size={14} />
            {p.sharing ? 'Condivisa' : 'Condividi'}
          </button>
        ) : (
          <span className="guest-badge">{p.access === 'edit' ? 'Puoi modificare' : 'Solo visione'}</span>
        )}
      </div>
      <div className="panel-sub">
        <span className="tab-text" aria-selected="true">
          Design
        </span>
        <ZoomMenu />
      </div>
      <div className="panel-scroll">
        {useEditor.getState().readOnly ? <ViewerInfo /> : els.length ? <SelectionProps board={p.board} els={els} /> : <BoardProps board={p.board} onExport={p.onExport} />}
      </div>
    </aside>
  )
}

function ViewerInfo() {
  return (
    <Section title="Solo visione">
      <p className="hint">Puoi guardare la lavagna, spostarti, fare zoom e usare il puntatore laser. Chi l'ha condivisa può darti il permesso di modificarla.</p>
    </Section>
  )
}

/* ---------- people ---------- */

function Participants({ awareness, following }: { awareness: Awareness | null; following: number | null }) {
  const peers = usePeers(awareness)
  const me = awareness?.getLocalState()
  const list = [
    ...(me ? [{ id: awareness!.clientID, name: `${peerName(me)} (tu)`, color: peerColor(me), self: true }] : []),
    ...peers.filter(({ state }) => state.user).map(({ id, state }) => ({ id, name: peerName(state), color: peerColor(state), self: false })),
  ]
  const shown = list.slice(0, 4)
  return (
    <div className="avatars">
      {shown.map((u) => (
        <Tip key={u.id} label={u.self ? u.name : following === u.id ? `Smetti di seguire ${u.name}` : `Segui ${u.name}`} side="bottom">
          <button
            type="button"
            className="avatar-btn"
            data-following={following === u.id || undefined}
            style={{ ['--ring' as string]: u.color }}
            disabled={u.self}
            onClick={() => commands.follow(following === u.id ? null : u.id)}
            aria-label={u.self ? u.name : `Segui ${u.name}`}
          >
            <Avatar name={u.name.replace(' (tu)', '')} color={u.color} />
          </button>
        </Tip>
      ))}
      {list.length > 4 && <span className="avatar more">+{list.length - 4}</span>}
    </div>
  )
}

function ZoomMenu() {
  const z = useEditor((s) => s.camera.z)
  return (
    <Menu>
      <MenuTrigger asChild>
        <button type="button" className="zoom-btn num" aria-label={`Zoom ${Math.round(z * 100)}%`}>
          {Math.round(z * 100)}%
          <ChevronDown size={12} />
        </button>
      </MenuTrigger>
      <MenuContent align="end">
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
      </MenuContent>
    </Menu>
  )
}

/* ---------- nothing selected: the board ---------- */

function BoardProps({ board, onExport }: { board: Board; onExport: () => void }) {
  const meta = board.getMeta()
  return (
    <>
      <Section title="Sfondo">
        <div className="bg-swatches" role="radiogroup" aria-label="Colore dello sfondo">
          {BACKGROUNDS.map((b) => (
            <Tip key={b.value} label={b.name}>
              <button type="button" role="radio" aria-checked={meta.background.toUpperCase() === b.value} aria-label={b.name} className="bg-swatch" style={{ background: b.value }} onClick={() => board.setMeta({ background: b.value })} />
            </Tip>
          ))}
          <label className="bg-swatch custom" title="Colore personalizzato">
            <span className="sr-only">Colore personalizzato</span>
            <input type="color" value={meta.background} onChange={(e) => board.setMeta({ background: e.target.value.toUpperCase() })} />
          </label>
        </div>
        <Segmented<Pattern>
          label="Motivo dello sfondo"
          value={meta.pattern}
          onChange={(pattern) => board.setMeta({ pattern })}
          options={[
            { value: 'none', label: 'Nessuno' },
            { value: 'dots', label: 'Puntini' },
            { value: 'grid', label: 'Griglia' },
            { value: 'lines', label: 'Righe' },
          ]}
        />
      </Section>
      <Section title="Esporta">
        <button type="button" className="btn btn-secondary btn-block" onClick={onExport}>
          Esporta lavagna…
        </button>
      </Section>
      <Section>
        <p className="hint">Seleziona qualcosa sulla lavagna per modificarne colore, dimensioni e posizione.</p>
      </Section>
    </>
  )
}

/* ---------- selection ---------- */

function SelectionProps({ board, els }: { board: Board; els: El[] }) {
  const single = els.length === 1 ? els[0] : null
  const editable = els.filter((e) => !e.locked)
  const apply = (fn: (el: El) => Partial<El> | null) => {
    const patches = new Map<string, Partial<El>>()
    for (const el of editable) {
      const p = fn(el)
      if (p) patches.set(el.id, p)
    }
    board.updateMany(patches)
  }
  const same = <T,>(get: (el: El) => T | undefined): T | null => {
    const vals = els.map(get).filter((v) => v !== undefined) as T[]
    return vals.length && vals.every((v) => v === vals[0]) ? vals[0] : null
  }
  const types = new Set(els.map((e) => e.type))
  const only = (t: El['type']) => types.size === 1 && types.has(t)
  const frame = union(els.map(frameBox))!

  const setFrame = (patch: Partial<{ x: number; y: number; w: number; h: number }>) => {
    if (single && single.type !== 'line') {
      if (patch.x !== undefined || patch.y !== undefined) {
        apply(() => ({ x: patch.x ?? single.x, y: patch.y ?? single.y }))
        return
      }
      const from = { x: single.x, y: single.y, w: single.w, h: single.h }
      const to = { ...from, w: Math.max(1, patch.w ?? single.w), h: Math.max(1, patch.h ?? single.h) }
      const next = { ...single, ...scaleElement({ ...single, rotation: 0 } as El, from, to), rotation: single.rotation } as El
      // Keep the top-left corner where it was.
      apply(() => ({ ...next, x: single.x, y: single.y, id: single.id }))
      return
    }
    const to = { ...frame, ...patch, w: Math.max(1, patch.w ?? frame.w), h: Math.max(1, patch.h ?? frame.h) }
    if (patch.x !== undefined || patch.y !== undefined) {
      const dx = to.x - frame.x
      const dy = to.y - frame.y
      apply((el) => ({ x: el.x + dx, y: el.y + dy }))
    } else apply((el) => scaleElement(el, frame, to))
  }

  const mainColor = same((el) => {
    const k = colorKey(el)
    return k ? ((el as unknown as Record<string, string>)[k] as string) : undefined
  })

  return (
    <>
      <Section>
        <div className="insp-kind">
          <strong>{single ? elementLabel(single) : `${els.length} elementi`}</strong>
          {single && elementLabel(single) !== TYPE_LABEL[single.type] && <span className="hint">{TYPE_LABEL[single.type]}</span>}
        </div>
      </Section>

      {els.length > 1 && (
        <Section title="Allinea">
          <div className="btn-row">
            <IconButton label="Allinea a sinistra" onClick={() => commands.align('left')}>
              <AlignStartVertical size={16} />
            </IconButton>
            <IconButton label="Centra orizzontalmente" onClick={() => commands.align('hcenter')}>
              <AlignCenterVertical size={16} />
            </IconButton>
            <IconButton label="Allinea a destra" onClick={() => commands.align('right')}>
              <AlignEndVertical size={16} />
            </IconButton>
            <IconButton label="Allinea in alto" onClick={() => commands.align('top')}>
              <AlignStartHorizontal size={16} />
            </IconButton>
            <IconButton label="Centra verticalmente" onClick={() => commands.align('vcenter')}>
              <AlignCenterHorizontal size={16} />
            </IconButton>
            <IconButton label="Allinea in basso" onClick={() => commands.align('bottom')}>
              <AlignEndHorizontal size={16} />
            </IconButton>
          </div>
          {els.length > 2 && (
            <div className="btn-row">
              <IconButton label="Distribuisci in orizzontale" onClick={() => commands.distribute('x')}>
                <AlignHorizontalDistributeCenter size={16} />
              </IconButton>
              <IconButton label="Distribuisci in verticale" onClick={() => commands.distribute('y')}>
                <AlignVerticalDistributeCenter size={16} />
              </IconButton>
            </div>
          )}
        </Section>
      )}

      <Section title="Posizione e dimensioni">
        <div className="grid2">
          <NumberField label="X" title="Posizione orizzontale" value={single ? single.x : frame.x} onChange={(x) => setFrame({ x })} />
          <NumberField label="Y" title="Posizione verticale" value={single ? single.y : frame.y} onChange={(y) => setFrame({ y })} />
          <NumberField label="L" title="Larghezza" min={1} value={single && single.type !== 'line' ? single.w : frame.w} onChange={(w) => setFrame({ w })} />
          <NumberField label="A" title="Altezza" min={1} value={single && single.type !== 'line' ? single.h : frame.h} onChange={(h) => setFrame({ h })} />
          {single && single.type !== 'line' && (
            <NumberField
              label={<RotateCw size={11} />}
              title="Rotazione in gradi"
              suffix="°"
              value={(((single.rotation * 180) / Math.PI) % 360 + 360) % 360}
              onChange={(deg) => apply((el) => ({ rotation: (deg * Math.PI) / 180, ...rotatedAround(el, (deg * Math.PI) / 180) }))}
            />
          )}
        </div>
      </Section>

      {only('text') || only('sticky') ? <TextProps els={els as (TextEl | Extract<El, { type: 'sticky' }>)[]} apply={apply} same={same} /> : null}

      {only('sticky') && (
        <Section title="Colore della nota">
          <Swatches label="Colore della nota" colors={STICKY_COLORS} value={same((e) => (e.type === 'sticky' ? e.color : undefined)) ?? ''} onChange={(color) => apply(() => ({ color }) as Partial<El>)} />
        </Section>
      )}

      {only('shape') && (
        <Section title="Riempimento">
          <Swatches label="Riempimento" colors={INK_COLORS} allowTransparent value={same((e) => (e.type === 'shape' ? e.fill : undefined)) ?? ''} onChange={(fill) => apply(() => ({ fill }) as Partial<El>)} />
        </Section>
      )}

      {mainColor !== undefined && !only('sticky') && types.size > 0 && [...types].every((t) => t !== 'image' && t !== 'stamp') && (
        <Section title={only('shape') || only('line') ? 'Contorno' : 'Colore'}>
          <Swatches
            label="Colore"
            colors={INK_COLORS}
            value={mainColor ?? ''}
            onChange={(c) =>
              apply((el) => {
                const k = colorKey(el)
                return k && el.type !== 'sticky' ? ({ [k]: c } as Partial<El>) : null
              })
            }
          />
          <StrokeWidth els={els} apply={apply} />
        </Section>
      )}

      {only('shape') && (
        <Section title="Forma">
          <Segmented<ShapeKind>
            label="Tipo di forma"
            value={(same((e) => (e.type === 'shape' ? e.shape : undefined)) ?? 'rect') as ShapeKind}
            onChange={(shape) => apply(() => ({ shape }) as Partial<El>)}
            options={[
              { value: 'rect', label: '▭', title: 'Rettangolo' },
              { value: 'ellipse', label: '◯', title: 'Ellisse' },
              { value: 'triangle', label: '△', title: 'Triangolo' },
              { value: 'diamond', label: '◇', title: 'Rombo' },
              { value: 'hexagon', label: '⬡', title: 'Esagono' },
              { value: 'star', label: '☆', title: 'Stella' },
            ]}
          />
          {same((e) => (e.type === 'shape' ? e.shape : undefined)) === 'rect' && (
            <NumberField label="◜" title="Raggio degli angoli" min={0} value={same((e) => (e.type === 'shape' ? e.radius : undefined))} onChange={(radius) => apply(() => ({ radius }) as Partial<El>)} />
          )}
        </Section>
      )}

      {only('line') && (
        <Section title="Frecce">
          <div className="btn-row">
            <IconButton label="Freccia all'inizio" on={!!same((e) => (e.type === 'line' ? e.arrowStart : undefined))} onClick={() => apply((e) => (e.type === 'line' ? { arrowStart: !e.arrowStart } : null) as Partial<El>)}>
              <MoveLeft size={16} />
            </IconButton>
            <IconButton label="Freccia alla fine" on={!!same((e) => (e.type === 'line' ? e.arrowEnd : undefined))} onClick={() => apply((e) => (e.type === 'line' ? { arrowEnd: !e.arrowEnd } : null) as Partial<El>)}>
              <MoveRight size={16} />
            </IconButton>
          </div>
        </Section>
      )}

      {only('stamp') && (
        <Section title="Reazione">
          <div className="stamps" role="radiogroup" aria-label="Reazione">
            {STAMPS.map((s) => (
              <button key={s} type="button" role="radio" aria-checked={same((e) => (e.type === 'stamp' ? e.emoji : undefined)) === s} className="stamp-btn" onClick={() => apply(() => ({ emoji: s }) as Partial<El>)}>
                {s}
              </button>
            ))}
          </div>
        </Section>
      )}

      <Section title="Livello">
        <div className="grid2">
          <NumberField label="%" title="Opacità" min={0} max={100} suffix="%" value={same((e) => Math.round(e.opacity * 100))} onChange={(v) => apply(() => ({ opacity: v / 100 }))} />
          <div className="btn-row">
            <IconButton label={els.every((e) => e.locked) ? 'Sblocca' : 'Blocca'} kbd="Ctrl+Maiusc+L" on={els.every((e) => e.locked)} onClick={() => commands.toggleLock()}>
              {els.every((e) => e.locked) ? <Lock size={16} /> : <Unlock size={16} />}
            </IconButton>
            <IconButton label="Nascondi" kbd="Ctrl+Maiusc+H" onClick={() => commands.toggleHide()}>
              {els.every((e) => e.hidden) ? <EyeOff size={16} /> : <Eye size={16} />}
            </IconButton>
            <IconButton label="Porta in primo piano" kbd="Ctrl+Maiusc+]" onClick={() => commands.order('front')}>
              <ArrowUpToLine size={16} />
            </IconButton>
            <IconButton label="Porta in fondo" kbd="Ctrl+Maiusc+[" onClick={() => commands.order('back')}>
              <ArrowDownToLine size={16} />
            </IconButton>
          </div>
        </div>
      </Section>

      <Section>
        <div className="btn-row stretch">
          <button type="button" className="btn btn-secondary" onClick={() => commands.duplicate()}>
            <CopyPlus size={14} /> Duplica
          </button>
          <button type="button" className="btn btn-secondary danger-text" onClick={() => commands.remove()}>
            <Trash2 size={14} /> Elimina
          </button>
        </div>
      </Section>
    </>
  )
}

/** Rotation from the inspector keeps the element's centre in place. */
function rotatedAround(el: El, _rotation: number): Partial<El> {
  const c = center(el)
  return { x: c.x - el.w / 2, y: c.y - el.h / 2 }
}

function StrokeWidth({ els, apply }: { els: El[]; apply: (fn: (el: El) => Partial<El> | null) => void }) {
  const widths = els.map((e) => (e.type === 'ink' || e.type === 'highlighter' ? e.size : e.type === 'shape' || e.type === 'line' ? e.strokeWidth : undefined))
  if (widths.some((w) => w === undefined)) return null
  const value = widths.every((w) => w === widths[0]) ? (widths[0] as number) : null
  return (
    <NumberField
      label="≡"
      title="Spessore"
      min={0.5}
      max={200}
      step={0.5}
      decimals={1}
      value={value}
      onChange={(v) => apply((e) => (e.type === 'ink' || e.type === 'highlighter' ? { size: v } : { strokeWidth: v }) as Partial<El>)}
    />
  )
}

function TextProps({ els, apply, same }: { els: El[]; apply: (fn: (el: El) => Partial<El> | null) => void; same: <T>(get: (el: El) => T | undefined) => T | null }) {
  const font = same((e) => (e.type === 'text' || e.type === 'sticky' ? e.font : undefined))
  const align = same((e) => (e.type === 'text' || e.type === 'sticky' ? e.align : undefined))
  const isText = els.every((e) => e.type === 'text')
  /** Text boxes that size themselves need their box recomputed when type changes. */
  const relayout = (el: El, patch: Partial<TextEl>): Partial<El> => {
    if (el.type !== 'text') return patch as Partial<El>
    const next = { ...el, ...patch }
    const l = layoutText(next.text || ' ', fontCss(next.font, next.fontSize, next.bold, next.italic), next.fontSize, next.fixedWidth ? next.w : null)
    return { ...patch, h: l.height, ...(next.fixedWidth ? {} : { w: Math.max(l.width, 4) }) } as Partial<El>
  }
  return (
    <Section title="Testo">
      <Segmented<FontKind>
        label="Carattere"
        value={(font ?? 'sans') as FontKind}
        onChange={(f) => apply((el) => relayout(el, { font: f }))}
        options={[
          { value: 'sans', label: <span style={{ fontFamily: FONT_STACK.sans }}>Sans</span> },
          { value: 'serif', label: <span style={{ fontFamily: FONT_STACK.serif }}>Serif</span> },
          { value: 'mono', label: <span style={{ fontFamily: FONT_STACK.mono }}>Mono</span> },
          { value: 'hand', label: <span style={{ fontFamily: FONT_STACK.hand, fontSize: 14 }}>Mano</span> },
        ]}
      />
      {isText && (
        <div className="grid2">
          <NumberField label="Aa" title="Dimensione del testo" min={1} max={2000} value={same((e) => (e.type === 'text' ? Math.round(e.fontSize * 10) / 10 : undefined))} decimals={1} onChange={(fontSize) => apply((el) => relayout(el, { fontSize }))} />
          <div className="btn-row">
            <IconButton label="Grassetto" on={!!same((e) => (e.type === 'text' ? e.bold : undefined))} onClick={() => apply((el) => (el.type === 'text' ? relayout(el, { bold: !el.bold }) : null))}>
              <Bold size={16} />
            </IconButton>
            <IconButton label="Corsivo" on={!!same((e) => (e.type === 'text' ? e.italic : undefined))} onClick={() => apply((el) => (el.type === 'text' ? relayout(el, { italic: !el.italic }) : null))}>
              <Italic size={16} />
            </IconButton>
          </div>
        </div>
      )}
      <Segmented<Align>
        label="Allineamento del testo"
        value={(align ?? 'left') as Align}
        onChange={(a) => apply(() => ({ align: a }) as Partial<El>)}
        options={[
          { value: 'left', label: <TextAlignStart size={14} />, title: 'A sinistra' },
          { value: 'center', label: <TextAlignCenter size={14} />, title: 'Al centro' },
          { value: 'right', label: <TextAlignEnd size={14} />, title: 'A destra' },
        ]}
      />
    </Section>
  )
}
