import { test } from 'node:test'
import assert from 'node:assert/strict'
import * as Y from 'yjs'
import { Board } from './doc.ts'
import type { LineEl, SectionEl, ShapeEl } from './types.ts'

const rect = (id: string, z: number): ShapeEl => ({ id, type: 'shape', shape: 'rect', x: 0, y: 0, w: 10, h: 10, rotation: 0, z, opacity: 1, fill: 'transparent', stroke: '#000000', strokeWidth: 1, radius: 0, dash: false })

test('a folder keeps its elements together under its top element, and disappears when emptied', () => {
  const board = new Board(new Y.Doc())
  board.add([rect('a', 0), rect('x', 1), rect('b', 2), rect('y', 3)])
  const g = board.createGroup(['a', 'b'])
  assert.deepEqual(
    board.all().map((el) => el.id),
    ['x', 'a', 'b', 'y'],
  )
  assert.equal(board.groupList().length, 1)
  assert.equal(board.groupInfo(g)!.name, 'Cartella 1')

  board.setGroup(['y'], g)
  assert.deepEqual(
    board.members(g).map((el) => el.id),
    ['a', 'b', 'y'],
  )

  board.renameGroup(g, '  Schizzi  ')
  assert.equal(board.groupInfo(g)!.name, 'Schizzi')

  board.setGroup(['a', 'b', 'y'], null)
  assert.equal(board.groupList().length, 0)
  assert.equal(board.groups.size, 0)
})

test('moving a shape drags its connectors along, in the same undo step', () => {
  const board = new Board(new Y.Doc())
  const b = { ...rect('b', 1), x: 300 }
  const line: LineEl = { id: 'l', type: 'line', x: 0, y: 0, w: 0, h: 0, rotation: 0, z: 2, opacity: 1, points: [0, 0, 0, 0], stroke: '#000000', strokeWidth: 0, dash: false, arrowStart: false, arrowEnd: true, from: 'a', to: 'b' }
  board.add([rect('a', 0), b, line])
  const ends = () => {
    const l = board.get('l') as LineEl
    return [l.x + l.points[0], l.y + l.points[1], l.x + l.points[2], l.y + l.points[3]]
  }
  // Attached on creation: from the square's right side to the other's left side.
  assert.deepEqual(ends(), [10, 5, 300, 5])
  board.undo.stopCapturing()
  board.update('b', { y: 100 })
  // Centres (5, 5) and (305, 105): the line leaves through the right side, enters through the left.
  assert.deepEqual(ends().map((v) => Math.round(v)), [10, 7, 300, 103])
  board.undo.undo()
  assert.deepEqual(ends(), [10, 5, 300, 5])
})

test('sections paint behind everything, largest first; bad eraser marks are refused', () => {
  const board = new Board(new Y.Doc())
  const section = (id: string, w: number, z: number): SectionEl => ({ id, type: 'section', x: 0, y: 0, w, h: w, rotation: 0, z, opacity: 1, fill: '#FFFFFF' })
  board.add([rect('r', 0), section('small', 50, 5), section('big', 500, 9)])
  assert.deepEqual(
    board.paintOrder().map((el) => el.id),
    ['big', 'small', 'r'],
  )
  board.add([{ ...rect('bad', 3), erase: [{ p: 'x' as unknown as number[], s: 1, a: 1 }] }])
  assert.equal(board.get('bad'), undefined)
})
