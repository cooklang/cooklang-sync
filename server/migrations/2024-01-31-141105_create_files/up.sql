CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  deleted BOOL DEFAULT 0 NOT NULL,
  chunk_ids VARCHAR NOT NULL
)
