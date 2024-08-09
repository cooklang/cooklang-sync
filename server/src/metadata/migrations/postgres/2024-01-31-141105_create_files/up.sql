CREATE TABLE file_records (
  id SERIAL PRIMARY KEY,
  user_id INTEGER NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  deleted BOOL DEFAULT FALSE NOT NULL,
  chunk_ids VARCHAR NOT NULL
)
