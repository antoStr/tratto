import { create } from 'zustand'
import type { Ruler } from './ink.ts'
import type { Camera, FontKind, ShapeKind } from './types.ts'

export type Tool = 'select' | 'hand' | 'pen' | 'highlighter' | 'eraser' | 'lasso' | 'shape' | 'line' | 'arrow' | 'text' | 'sticky' | 'stamp' | 'laser'

export interface Pen {
  color: string
  size: number
}

/** Preferences kept on this device. */
export interface Prefs {
  name: string
  theme: 'system' | 'light' | 'dark'
  pens: Pen[]
  highlighter: Pen
  eraser: { mode: 'stroke' | 'precise'; size: number }
  inkToShape: boolean
  fingerDraw: boolean
  wheel: 'pan' | 'zoom'
  shapeStyle: { fill: string; stroke: string; strokeWidth: number }
  text: { color: string; fontSize: number; font: FontKind }
  stickyColor: string
  stamp: string
  lastShape: ShapeKind
}

const DEFAULT_PREFS: Prefs = {
  name: '',
  theme: 'system',
  pens: [
    { color: '#1E1E1E', size: 4 },
    { color: '#1971C2', size: 4 },
    { color: '#E03131', size: 4 },
    { color: '#2F9E44', size: 8 },
  ],
  highlighter: { color: '#FFE066', size: 22 },
  eraser: { mode: 'stroke', size: 24 },
  inkToShape: false,
  fingerDraw: false,
  wheel: 'pan',
  shapeStyle: { fill: 'transparent', stroke: '#1E1E1E', strokeWidth: 3 },
  text: { color: '#1E1E1E', fontSize: 24, font: 'sans' },
  stickyColor: '#FFF3A3',
  stamp: '👍',
  lastShape: 'rect',
}

const PREFS_KEY = 'tratto.prefs.v1'

function loadPrefs(): Prefs {
  try {
    const saved = JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}')
    return { ...DEFAULT_PREFS, ...saved }
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
  ui: boolean
  readOnly: boolean
  /** Awareness client id whose view we follow, if any. */
  following: number | null
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
  ui: true,
  readOnly: false,
  following: null,
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

export const PEN_SIZES = [2, 4, 8, 14]
export const HIGHLIGHTER_SIZES = [14, 22, 32]
export const ERASER_SIZES = [12, 24, 48]
