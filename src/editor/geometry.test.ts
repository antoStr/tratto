import { test } from 'node:test'
import assert from 'node:assert/strict'
import { aabb, hitTest, pointInPolygon, scaleElement, segmentDistance } from './geometry.ts'
import { recognize, rulerSnapper, strokeOutline, thinPoints } from './ink.ts'
import type { InkEl, ShapeEl } from './types.ts'

const flat = (pts: [number, number][]) => pts.flatMap(([x, y]) => [x, y, 0.5])
const circle = (cx: number, cy: number, r: number) => {
  const pts: [number, number][] = []
  for (let a = 0; a <= Math.PI * 2.02; a += 0.06) pts.push([cx + Math.cos(a) * r, cy + Math.sin(a) * r * 0.8])
  return pts
}
const polyline = (corners: [number, number][], step = 4) => {
  const pts: [number, number][] = []
  for (let i = 0; i < corners.length - 1; i++) {
    const [x1, y1] = corners[i]
    const [x2, y2] = corners[i + 1]
    const n = Math.max(1, Math.round(Math.hypot(x2 - x1, y2 - y1) / step))
    for (let k = 0; k < n; k++) pts.push([x1 + ((x2 - x1) * k) / n, y1 + ((y2 - y1) * k) / n])
  }
  pts.push(corners[corners.length - 1])
  return pts
}

test('ink to shape recognises the basic shapes and leaves scribbles alone', () => {
  assert.equal(recognize(flat(polyline([[0, 0], [200, 40]])))?.kind, 'line')
  assert.equal(recognize(flat(circle(100, 100, 80)))?.kind, 'ellipse')
  assert.equal(recognize(flat(polyline([[0, 0], [200, 0], [200, 120], [0, 120], [0, 2]])))?.kind, 'rect')
  assert.equal(recognize(flat(polyline([[100, 0], [200, 160], [0, 160], [98, 3]])))?.kind, 'triangle')
  const scribble: [number, number][] = []
  for (let t = 0; t < 1; t += 0.01) scribble.push([t * 300, Math.sin(t * 40) * 30])
  assert.equal(recognize(flat(scribble)), null)
})

test('a stroke written zoomed in looks the same on screen as at 100%', () => {
  // A 40 px handwriting stroke with a 4 px pen; at zoom z it is 1/z the size in board units.
  const stroke = (zoom: number) => {
    const pts: number[] = []
    for (let i = 0; i <= 20; i++) pts.push((i * 2) / zoom, (Math.sin((i / 20) * Math.PI) * 10) / zoom, -1)
    return strokeOutline(pts, 4 / zoom, false).map(([x, y]) => [x * zoom, y * zoom])
  }
  const base = stroke(1)
  for (const zoom of [4, 16, 32]) {
    const out = stroke(zoom)
    assert.equal(out.length, base.length, `zoom ${zoom}: same outline`)
    out.forEach(([x, y], i) => assert.ok(Math.hypot(x - base[i][0], y - base[i][1]) < 1e-6, `zoom ${zoom}: point ${i}`))
  }
  // A tap is a dot the size of the pen, not a dash.
  const dot = strokeOutline([10, 10, -1], 4 / 32, false)
  assert.ok(Math.max(...dot.map(([x, y]) => Math.hypot(x - 10, y - 10))) * 32 < 3)
  // Shapes are recognised at any zoom: the same circle on screen, drawn at 1600%.
  assert.equal(recognize(flat(circle(100, 100, 80)).map((v, i) => (i % 3 === 2 ? v : v / 16)), 1 / 16)?.kind, 'ellipse')
  assert.equal(recognize(flat(polyline([[0, 0], [200, 40]])).map((v, i) => (i % 3 === 2 ? v : v / 16)), 1 / 16)?.kind, 'line')
})

test('hit testing respects stroke width, fill and rotation', () => {
  const ink: InkEl = { id: 'a', type: 'ink', x: 0, y: 0, w: 100, h: 0, rotation: 0, z: 0, opacity: 1, points: [0, 0, 0.5, 100, 0, 0.5], color: '#000', size: 10 }
  assert.ok(hitTest(ink, 50, 4, 0))
  assert.ok(!hitTest(ink, 50, 12, 0))
  const box: ShapeEl = { id: 'b', type: 'shape', shape: 'rect', x: 0, y: 0, w: 100, h: 50, rotation: 0, z: 0, opacity: 1, fill: 'transparent', stroke: '#000', strokeWidth: 2, radius: 0, dash: false }
  assert.ok(hitTest(box, 0, 25, 2), 'edge of an empty rectangle')
  assert.ok(!hitTest(box, 50, 25, 2), 'inside of an empty rectangle is click-through')
  assert.ok(hitTest({ ...box, fill: '#fff' }, 50, 25, 2), 'inside of a filled rectangle')
  const turned = { ...box, rotation: Math.PI / 2 }
  // Rotated 90° about its centre (50, 25): the box now spans x 25..75, y -25..75.
  assert.ok(hitTest({ ...turned, fill: '#fff' }, 50, 70, 1))
  assert.ok(!hitTest({ ...turned, fill: '#fff' }, 90, 25, 1))
  const b = aabb(turned)
  // Stroke padding (width / 2 + 1) on every side, then rotated: 54 × 104.
  assert.ok(Math.abs(b.w - 54) < 0.01 && Math.abs(b.h - 104) < 0.01)
})

test('scaling a stroke scales its points and thickness', () => {
  const ink: InkEl = { id: 'a', type: 'ink', x: 10, y: 10, w: 100, h: 50, rotation: 0, z: 0, opacity: 1, points: [0, 0, 0.5, 100, 50, 0.5], color: '#000', size: 4 }
  const p = scaleElement(ink, { x: 10, y: 10, w: 100, h: 50 }, { x: 10, y: 10, w: 200, h: 100 }) as Partial<InkEl>
  assert.deepEqual(p.points, [0, 0, 0.5, 200, 100, 0.5])
  assert.equal(p.size, 8)
  assert.equal(p.x, 10)
  // A thin stroke written zoomed in stays thin (it used to jump to 0.5 units).
  const thin = scaleElement({ ...ink, size: 0.06 }, { x: 10, y: 10, w: 100, h: 50 }, { x: 10, y: 10, w: 110, h: 55 }) as Partial<InkEl>
  assert.ok(Math.abs(thin.size! - 0.066) < 1e-9)
})

test('geometry helpers', () => {
  assert.equal(segmentDistance(0, 0, 10, 10, 0, 10, 10, 0), 0, 'crossing segments')
  assert.equal(segmentDistance(0, 0, 10, 0, 0, 5, 10, 5), 5)
  assert.ok(pointInPolygon(5, 5, [0, 0, 10, 0, 10, 10, 0, 10]))
  assert.ok(!pointInPolygon(15, 5, [0, 0, 10, 0, 10, 10, 0, 10]))
  assert.equal(thinPoints([0, 0, 1, 0.1, 0, 1, 0.2, 0, 1, 10, 0, 1], 1).length, 6)
})

test('ruler: strokes that start at its edge are pulled onto a straight line', () => {
  const ruler = { visible: true, x: 500, y: 300, angle: 0 }
  // Top edge is at y = 300 - 42 = 258; start 10px above it.
  const snap = rulerSnapper(ruler, 400, 248, 2)
  assert.ok(snap)
  const a = snap(420, 230)
  const b = snap(700, 262)
  assert.equal(a.y, b.y, 'all points land on the same line')
  assert.equal(a.y, 256)
  assert.equal(rulerSnapper(ruler, 400, 120, 2), null, 'far from the ruler: free drawing')
  assert.equal(rulerSnapper({ ...ruler, visible: false }, 400, 248, 2), null)
})
