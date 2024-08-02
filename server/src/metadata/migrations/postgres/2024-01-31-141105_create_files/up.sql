CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  user_id INTEGER NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  deleted BOOL DEFAULT FALSE NOT NULL,
  chunk_ids VARCHAR NOT NULL
)
