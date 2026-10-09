import { create } from 'zustand'
import type { Ruler } from './ink.ts'
import { DEFAULT_META, type BoardMeta, type Camera, type FontKind, type ShapeKind } from './types.ts'

export type Tool = 'select' | 'hand' | 'pen' | 'highlighter' | 'eraser' | 'lasso' | 'shape' | 'line' | 'arrow' | 'text' | 'sticky' | 'stamp' | 'laser' | 'section' | 'tape' | 'comment'

/** Shape tool choices: every ShapeKind plus the rounded rectangle (a rect with radius). */
export type ShapeTool = ShapeKind | 'roundRect'

export interface Pen {
  color: string
  /** Width on screen, in px. */
  size: number
  /** 0.1–1: how strong the ink is (Paint's opacity). */
  opacity?: number
}

/** 'stroke' removes whole strokes and shapes; 'pixel' erases only where it passes, like Paint. */
export interface EraserPrefs {
  mode: 'stroke' | 'pixel'
  /** Diameter on screen, in px. */
  size: number
  /** 0.1–1: how much one pass removes (pixel mode). */
  strength: number
}

/** Preferences kept on this device. */
export interface Prefs {
  name: string
  theme: 'system' | 'light' | 'dark'
  pens: Pen[]
  highlighter: Pen
  /** Washi tape colour and width on screen. */
  tape: Pen
  eraser: EraserPrefs
  inkToShape: boolean
  /** How much tremor is smoothed out of freehand strokes. */
  inkSmoothing: 'low' | 'medium' | 'high'
  /** Use the pen's pressure for line width. */
  pressure: boolean
  fingerDraw: boolean
  wheel: 'pan' | 'zoom'
  shapeStyle: { fill: string; stroke: string; strokeWidth: number }
  text: { color: string; fontSize: number; font: FontKind }
  stickyColor: string
  stamp: string
  lastShape: ShapeTool
  /* Settings dialog */
  /** Whole-window zoom of the desktop app, 1 = 100%. */
  uiScale: number
  highContrast: boolean
  bigHandles: boolean
  /** Brand colour used for selection, buttons and focus. */
  accent: string
  toolbarPos: 'bottom' | 'top'
  leftPanel: boolean
  rightPanel: boolean
  minimap: boolean
  /** Focus mode: both side panels hidden, only the toolbar on the board (Ctrl+\). */
  focus: boolean
  /** Interface text size, 1 = Figma's 11 px. */
  textScale: number
  /** Background of new boards. */
  newBoard: BoardMeta
}

export const DEFAULT_ACCENT = '#0D99FF'

const DEFAULT_PREFS: Prefs = {
  name: '',
  theme: 'system',
  pens: [
    { color: '#1E1E1E', size: 4, opacity: 1 },
    { color: '#1971C2', size: 4, opacity: 1 },
    { color: '#E03131', size: 4, opacity: 1 },
    { color: '#2F9E44', size: 8, opacity: 1 },
  ],
  highlighter: { color: '#FFE066', size: 22 },
  tape: { color: '#FFB3C7', size: 28 },
  eraser: { mode: 'pixel', size: 24, strength: 1 },
  inkToShape: false,
  inkSmoothing: 'medium',
  pressure: true,
  fingerDraw: false,
  wheel: 'pan',
  shapeStyle: { fill: 'transparent', stroke: '#1E1E1E', strokeWidth: 3 },
  text: { color: '#1E1E1E', fontSize: 24, font: 'sans' },
  stickyColor: '#FFF3A3',
  stamp: '👍',
  lastShape: 'rect',
  uiScale: 1,
  highContrast: false,
  bigHandles: false,
  accent: DEFAULT_ACCENT,
  toolbarPos: 'bottom',
  leftPanel: true,
  rightPanel: true,
  minimap: true,
  focus: false,
  textScale: 1,
  newBoard: DEFAULT_META,
}

const PREFS_KEY = 'tratto.prefs.v1'

function loadPrefs(): Prefs {
  try {
    const saved = JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}')
    const prefs: Prefs = { ...DEFAULT_PREFS, ...saved }
    // Older versions: 'precise' eraser, no strength.
    const eraser = { ...DEFAULT_PREFS.eraser, ...saved.eraser }
    if (eraser.mode !== 'stroke') eraser.mode = 'pixel'
    return { ...prefs, eraser }
  } catch {
    return DEFAULT_PREFS
  }
}

export interface EditorState {
  tool: Tool
  pen: number
  prefs: Prefs
  selection: string[]
  camera: Camera
  ruler: Ruler
  editingId: string | null
  /** Comment whose thread is open. */
  commentId: string | null
  readOnly: boolean
  /** Awareness client id whose view we follow, if any. */
  following: number | null
  /** The board has visible content but none of it is on screen. */
  lost: boolean
  setTool: (tool: Tool) => void
  setPrefs: (patch: Partial<Prefs>) => void
  select: (ids: string[]) => void
  setCamera: (cam: Camera) => void
  setRuler: (patch: Partial<Ruler>) => void
}

export const useEditor = create<EditorState>((set, get) => ({
  tool: 'pen',
  pen: 0,
  prefs: loadPrefs(),
  selection: [],
  camera: { x: 0, y: 0, z: 1 },
  ruler: { visible: false, x: 600, y: 400, angle: 0 },
  editingId: null,
  commentId: null,
  readOnly: false,
  following: null,
  lost: false,
  setTool: (tool) => set({ tool, editingId: null, ...(tool !== 'select' && tool !== 'lasso' ? { selection: [] } : {}) }),
  setPrefs: (patch) => {
    const prefs = { ...get().prefs, ...patch }
    set({ prefs })
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(prefs))
    } catch {
      /* private mode: preferences last for this session only */
    }
  },
  select: (selection) => set({ selection }),
  setCamera: (camera) => set({ camera }),
  setRuler: (patch) => set({ ruler: { ...get().ruler, ...patch } }),
}))

/** Ctrl+\: hides both panels, or brings both back when none is showing. */
export const toggleFocus = () => {
  const { prefs, setPrefs } = useEditor.getState()
  const showing = !prefs.focus && (prefs.leftPanel || prefs.rightPanel)
  setPrefs(showing ? { focus: true } : { focus: false, leftPanel: true, rightPanel: true })
}

/** This device's id for votes, kept across visits so a reconnect doesn't give fresh votes. */
let voter: string | null = null
export function voterId() {
  if (voter) return voter
  try {
    voter = localStorage.getItem('tratto.voter')
    if (!voter) localStorage.setItem('tratto.voter', (voter = crypto.randomUUID().slice(0, 12)))
  } catch {
    voter = crypto.randomUUID().slice(0, 12)
  }
  return voter
}

export const toggleMinimap = () => {
  const s = useEditor.getState()
  s.setPrefs({ minimap: !s.prefs.minimap })
}

/** Quick picks shown next to the size sliders. */
export const PEN_SIZES = [2, 4, 8, 14]
export const HIGHLIGHTER_SIZES = [14, 22, 32]
export const ERASER_SIZES = [12, 24, 48]
