CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  jid INTEGER,
  deleted BOOL DEFAULT 0 NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  modified_at DATETIME NOT NULL,
  size BIGINT NOT NULL
)
