import { uid } from './doc.ts'
import { fontCss, layoutText } from './render.ts'
import { STICKY_COLORS, type Align, type El, type LineEl, type ShapeEl, type StickyEl, type TextEl } from './types.ts'

export interface Template {
  id: string
  name: string
  description: string
  build(): El[]
}

const DARK = '#1E1E1E'
const BASE = { rotation: 0, z: 0, opacity: 1 }

interface TextOpts {
  size?: number
  bold?: boolean
  color?: string
  align?: Align
  /** Fixed width: the text wraps at this width. */
  w?: number
}

function text(x: number, y: number, str: string, o: TextOpts = {}): TextEl {
  const size = o.size ?? 16
  const bold = o.bold ?? false
  const lay = layoutText(str, fontCss('sans', size, bold, false), size, o.w ?? null)
  return {
    id: uid(), ...BASE, type: 'text', x, y, w: Math.ceil(o.w ?? lay.width) + 1, h: lay.height, text: str, color: o.color ?? DARK,
    fontSize: size, font: 'sans', align: o.align ?? 'left', bold, italic: false, fixedWidth: o.w !== undefined,
  }
}

/** Text whose box is centred on (cx, cy). */
function label(cx: number, cy: number, str: string, o: TextOpts = {}): TextEl {
  const t = text(0, 0, str, { align: 'center', ...o })
  t.x = cx - t.w / 2
  t.y = cy - t.h / 2
  return t
}

function shape(kind: ShapeEl['shape'], x: number, y: number, w: number, h: number, fill: string, stroke: string, strokeWidth: number, radius = 0): ShapeEl {
  return { id: uid(), ...BASE, type: 'shape', shape: kind, x, y, w, h, fill, stroke, strokeWidth, radius, dash: false }
}

/** Figma-style white card. */
const frame = (x: number, y: number, w: number, h: number) => shape('rect', x, y, w, h, '#FFFFFF', '#E6E6E6', 1, 12)

/** A shape with its label written inside it (FigJam), so they move together. */
const withText = (el: ShapeEl, text: string): ShapeEl => ({ ...el, text, font: 'sans' })

function sticky(x: number, y: number, color: string, str = ''): StickyEl {
  return { id: uid(), ...BASE, type: 'sticky', x, y, w: 200, h: 200, text: str, color, font: 'hand', align: 'center' }
}

function line(x1: number, y1: number, x2: number, y2: number, arrow: boolean, color = '#757575', width = 2): LineEl {
  return {
    id: uid(), ...BASE, type: 'line', x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1),
    points: [x1 - Math.min(x1, x2), y1 - Math.min(y1, y2), x2 - Math.min(x1, x2), y2 - Math.min(y1, y2)],
    stroke: color, strokeWidth: width, dash: false, arrowStart: false, arrowEnd: arrow,
  }
}

/** Assigns increasing z in paint order. */
const finish = (els: El[]): El[] => els.map((e, i) => ({ ...e, z: i }))

/** Frames side by side with a bold header each, centred on the origin. */
function columns(titles: string[], colW: number, colH: number, gap: number, headerSize: number, perColumn?: (i: number, x: number, y: number) => El[]): El[] {
  const out: El[] = []
  const x0 = -(titles.length * colW + (titles.length - 1) * gap) / 2
  const y0 = -colH / 2
  titles.forEach((title, i) => {
    const x = x0 + i * (colW + gap)
    out.push(frame(x, y0, colW, colH), text(x + 20, y0 + 20, title, { size: headerSize, bold: true }))
    if (perColumn) out.push(...perColumn(i, x, y0))
  })
  return out
}

/** Points on an ellipse around the origin, starting at the top. */
const ring = (n: number, rx: number, ry: number) =>
  Array.from({ length: n }, (_, i) => {
    const a = -Math.PI / 2 + (i * 2 * Math.PI) / n
    return { x: Math.cos(a) * rx, y: Math.sin(a) * ry }
  })

const [YELLOW, GREEN, BLUE, PINK, VIOLET, ORANGE] = STICKY_COLORS

export const TEMPLATES: Template[] = [
  {
    id: 'brainstorm',
    name: 'Brainstorming',
    description: 'Un tema al centro e tante note attorno per raccogliere idee.',
    build() {
      const out: El[] = [text(-150, -560, 'Brainstorming', { size: 28, bold: true }), sticky(-100, -100, YELLOW, 'Tema')]
      const colors = [GREEN, BLUE, PINK, VIOLET, ORANGE]
      ring(8, 460, 340).forEach((p, i) => out.push(sticky(p.x - 100, p.y - 100, colors[i % colors.length])))
      return finish(out)
    },
  },
  {
    id: 'kanban',
    name: 'Kanban',
    description: 'Tre colonne: da fare, in corso, fatto.',
    build() {
      const examples = [[YELLOW, YELLOW, YELLOW], [BLUE, BLUE], [GREEN, GREEN]]
      const texts = [['Idea 1', 'Idea 2', 'Idea 3'], ['Attività 1', 'Attività 2'], ['Completata 1', 'Completata 2']]
      return finish(
        columns(['Da fare', 'In corso', 'Fatto'], 248, 740, 32, 24, (i, x, y) =>
          examples[i].map((c, j) => sticky(x + 24, y + 72 + j * 216, c, texts[i][j])),
        ),
      )
    },
  },
  {
    id: 'swot',
    name: 'Analisi SWOT',
    description: 'Punti di forza, punti deboli, opportunità e minacce.',
    build() {
      const quads: [string, string, string][] = [
        ['Punti di forza', '#E6F6EA', '#B7E4C2'],
        ['Punti deboli', '#FDE8E8', '#F5BFBF'],
        ['Opportunità', '#E7F1FD', '#B9D7F7'],
        ['Minacce', '#FFF0DC', '#F7D3A3'],
      ]
      const W = 440
      const H = 320
      const gap = 24
      const out: El[] = []
      quads.forEach(([title, fill, stroke], i) => {
        const x = -W - gap / 2 + (i % 2) * (W + gap)
        const y = -H - gap / 2 + Math.floor(i / 2) * (H + gap)
        out.push(shape('rect', x, y, W, H, fill, stroke, 1, 16), text(x + 24, y + 20, title, { size: 24, bold: true }))
      })
      return finish(out)
    },
  },
  {
    id: 'retro',
    name: 'Retrospettiva',
    description: 'Cosa è andato bene, cosa migliorare e le azioni.',
    build: () => finish(columns(['Cosa è andato bene', 'Cosa migliorare', 'Azioni'], 300, 680, 32, 22)),
  },
  {
    id: 'mindmap',
    name: 'Mappa mentale',
    description: 'Un\'idea centrale con sei rami da sviluppare.',
    build() {
      const pts = ring(6, 440, 280)
      const colors = [BLUE, GREEN, PINK, VIOLET, ORANGE, YELLOW]
      // Text inside the shapes and connectors attached to them: a branch dragged away keeps its line.
      const centre = withText(shape('ellipse', -120, -64, 240, 128, '#E7F1FD', '#1971C2', 2), 'Idea centrale')
      const out: El[] = []
      const branches = pts.map((p, i) => {
        const w = 170
        const h = 64
        const b = i % 2 ? shape('ellipse', p.x - w / 2, p.y - h / 2, w, h, colors[i], '#757575', 1.5) : shape('rect', p.x - w / 2, p.y - h / 2, w, h, colors[i], '#757575', 1.5, 16)
        out.push({ ...line(0, 0, p.x, p.y, false), from: centre.id, to: b.id })
        return withText(b, `Ramo ${i + 1}`)
      })
      out.push(centre, ...branches)
      return finish(out)
    },
  },
  {
    id: 'week',
    name: 'Planner settimanale',
    description: 'Sette colonne, da lunedì a domenica.',
    build: () => finish(columns(['Lunedì', 'Martedì', 'Mercoledì', 'Giovedì', 'Venerdì', 'Sabato', 'Domenica'], 220, 620, 16, 22)),
  },
  {
    id: 'flow',
    name: 'Diagramma di flusso',
    description: 'Inizio, passo, decisione e due esiti collegati da frecce.',
    build() {
      const sw = 2
      const start = shape('ellipse', -560, -40, 160, 80, GREEN, '#2F9E44', sw)
      const step = shape('rect', -310, -40, 180, 80, BLUE, '#1971C2', sw, 12)
      const decision = shape('diamond', -60, -80, 240, 160, YELLOW, '#F5B700', sw)
      const yes = shape('rect', 300, -210, 200, 80, GREEN, '#2F9E44', sw, 12)
      const no = shape('rect', 300, 130, 200, 80, PINK, '#C2255C', sw, 12)
      const cy = (s: ShapeEl) => s.y + s.h / 2
      const link = (a: ShapeEl, b: ShapeEl, x1: number, y1: number, x2: number, y2: number) => ({ ...line(x1, y1, x2, y2, true), from: a.id, to: b.id })
      return finish([
        link(start, step, start.x + start.w, 0, step.x, 0),
        link(step, decision, step.x + step.w, 0, decision.x, 0),
        link(decision, yes, decision.x + decision.w, 0, yes.x, cy(yes)),
        link(decision, no, decision.x + decision.w, 0, no.x, cy(no)),
        withText(start, 'Inizio'),
        withText(step, 'Passo'),
        withText(decision, 'Decisione?'),
        withText(yes, 'Esito A'),
        withText(no, 'Esito B'),
        label(250, -110, 'Sì', { size: 16, bold: true, color: '#2F9E44' }),
        label(250, 110, 'No', { size: 16, bold: true, color: '#C2255C' }),
      ])
    },
  },
]
