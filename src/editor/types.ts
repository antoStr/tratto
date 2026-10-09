export type ShapeKind = 'rect' | 'ellipse' | 'triangle' | 'diamond' | 'star' | 'hexagon' | 'polygon'
export type FontKind = 'sans' | 'rounded' | 'geometric' | 'condensed' | 'serif' | 'book' | 'display' | 'mono' | 'hand' | 'print' | 'marker'
export type Align = 'left' | 'center' | 'right'

interface Base {
  id: string
  x: number
  y: number
  w: number
  h: number
  rotation: number
  z: number
  opacity: number
  locked?: boolean
  hidden?: boolean
  name?: string
  /** Folder (Figma-style group) the element belongs to. */
  groupId?: string
}

/** A folder in the layers panel. Membership lives on the elements (`groupId`). */
export interface GroupInfo {
  id: string
  name: string
}

/** Freehand stroke. `points` is flat [x, y, pressure, …] relative to (x, y); w/h is the points' bounding box. */
export interface InkEl extends Base {
  type: 'ink' | 'highlighter'
  points: number[]
  color: string
  size: number
}

export interface ShapeEl extends Base {
  type: 'shape'
  shape: ShapeKind
  fill: string
  stroke: string
  strokeWidth: number
  radius: number
  dash: boolean
  /** Only for 'polygon': flat [x, y, …] normalised to 0..1 inside the box. */
  points?: number[]
}

/** Straight line or arrow. `points` is [x1, y1, x2, y2] relative to (x, y). */
export interface LineEl extends Base {
  type: 'line'
  points: number[]
  stroke: string
  strokeWidth: number
  dash: boolean
  arrowStart: boolean
  arrowEnd: boolean
}

export interface TextEl extends Base {
  type: 'text'
  text: string
  color: string
  fontSize: number
  font: FontKind
  align: Align
  bold: boolean
  italic: boolean
  /** When false the box grows with the text; when true it wraps at `w`. */
  fixedWidth: boolean
}

export interface StickyEl extends Base {
  type: 'sticky'
  text: string
  color: string
  font: FontKind
  align: Align
}

export interface ImageEl extends Base {
  type: 'image'
  fileId: string
}

export interface StampEl extends Base {
  type: 'stamp'
  emoji: string
}

export type El = InkEl | ShapeEl | LineEl | TextEl | StickyEl | ImageEl | StampEl
export type ElType = El['type']

export type Pattern = 'none' | 'dots' | 'grid' | 'lines' | 'graph' | 'isometric'
export const PATTERNS: Pattern[] = ['none', 'dots', 'grid', 'lines', 'graph', 'isometric']
export interface BoardMeta {
  /** Spacing of the background pattern in board units. */
  gridSize: number
  background: string
  pattern: Pattern
}

export interface Box {
  x: number
  y: number
  w: number
  h: number
}

/** screen = world * z + (x, y) */
export interface Camera {
  x: number
  y: number
  z: number
}

export const INK_COLORS = ['#1E1E1E', '#757575', '#FFFFFF', '#E03131', '#F76707', '#F5B700', '#2F9E44', '#0C8599', '#1971C2', '#6741D9', '#C2255C', '#8B5A2B']
export const HIGHLIGHT_COLORS = ['#FFE066', '#8CE99A', '#74C0FC', '#FCC2D7', '#FFC078', '#D0BFFF']
export const STICKY_COLORS = ['#FFF3A3', '#C9F2C7', '#C7E5FF', '#FFD1E3', '#E2D4FF', '#FFDDB8', '#E9E9E9']
export const BACKGROUNDS = [
  { name: 'Grigio chiaro', value: '#F5F5F5' },
  { name: 'Bianco', value: '#FFFFFF' },
  { name: 'Carta', value: '#FAF7F0' },
  { name: 'Ardesia', value: '#26292E' },
  { name: 'Lavagna verde', value: '#23392F' },
  { name: 'Blu notte', value: '#1B2A41' },
]

/** Every face but serif and mono (kept for existing boards) ships with the app, so guests see the same text. */
export const FONT_STACK: Record<FontKind, string> = {
  sans: '"Inter Variable", "Segoe UI", system-ui, sans-serif',
  rounded: '"Nunito Variable", "Segoe UI", system-ui, sans-serif',
  geometric: '"Montserrat Variable", "Segoe UI", system-ui, sans-serif',
  condensed: '"Oswald Variable", "Arial Narrow", sans-serif',
  serif: 'Georgia, Cambria, "Times New Roman", serif',
  book: '"Lora Variable", Georgia, serif',
  display: '"Playfair Display Variable", Georgia, serif',
  mono: '"Cascadia Code", Consolas, "SF Mono", monospace',
  hand: '"Caveat Variable", "Segoe Print", "Comic Sans MS", cursive',
  print: '"Patrick Hand", "Segoe Print", "Comic Sans MS", cursive',
  marker: '"Permanent Marker", "Segoe Print", "Comic Sans MS", cursive',
}

/** The font menu, in order: the name shown and its group. */
export const FONTS: { kind: FontKind; name: string; group: string }[] = [
  { kind: 'sans', name: 'Inter', group: 'Senza grazie' },
  { kind: 'rounded', name: 'Nunito', group: 'Senza grazie' },
  { kind: 'geometric', name: 'Montserrat', group: 'Senza grazie' },
  { kind: 'condensed', name: 'Oswald', group: 'Senza grazie' },
  { kind: 'serif', name: 'Georgia', group: 'Con grazie' },
  { kind: 'book', name: 'Lora', group: 'Con grazie' },
  { kind: 'display', name: 'Playfair Display', group: 'Con grazie' },
  { kind: 'hand', name: 'Caveat', group: 'Scritti a mano' },
  { kind: 'print', name: 'Patrick Hand', group: 'Scritti a mano' },
  { kind: 'marker', name: 'Permanent Marker', group: 'Scritti a mano' },
  { kind: 'mono', name: 'Monospaziato', group: 'Altri' },
]

export const DEFAULT_META: BoardMeta = { background: '#F5F5F5', pattern: 'dots', gridSize: 24 }
export const GRID_SIZES = [12, 24, 48]
