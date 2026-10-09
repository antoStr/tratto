import { test } from 'node:test'
import assert from 'node:assert/strict'
import * as Y from 'yjs'
import { Board } from './doc.ts'
import type { ShapeEl } from './types.ts'

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
