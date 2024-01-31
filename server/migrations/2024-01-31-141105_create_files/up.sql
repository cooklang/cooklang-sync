CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  chunk_ids VARCHAR NOT NULL,
  format CHARACTER(1) NOT NULL
)
