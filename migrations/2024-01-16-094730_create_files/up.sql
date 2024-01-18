CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  jid INTEGER NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  format CHARACTER(1) NOT NULL,
  modified_at TEXT,
  size INTEGER,
  created_at VARCHAR NOT NULL
)
