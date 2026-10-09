import { DatabaseSync } from 'node:sqlite'
import { mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

export interface BoardRow {
  id: string
  title: string
  created_at: number
  updated_at: number
  has_thumb: number
}

export function openDb(file: string) {
  if (file !== ':memory:') mkdirSync(dirname(file), { recursive: true })
  const db = new DatabaseSync(file)
  db.exec(`
    PRAGMA journal_mode = WAL;
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS boards (
      id TEXT PRIMARY KEY,
      title TEXT NOT NULL,
      ydoc BLOB,
      thumbnail BLOB,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
    -- Keyed per board so duplicated or imported boards keep the same file ids.
    CREATE TABLE IF NOT EXISTS files (
      board_id TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
      id TEXT NOT NULL,
      mime TEXT NOT NULL,
      data BLOB NOT NULL,
      created_at INTEGER NOT NULL,
      PRIMARY KEY (board_id, id)
    );
    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
  `)
  return db
}

export type Db = ReturnType<typeof openDb>
