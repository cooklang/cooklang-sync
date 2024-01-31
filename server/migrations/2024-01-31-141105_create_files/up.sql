CREATE TABLE file_records (
  id INTEGER PRIMARY KEY NOT NULL,
  deleted BOOL DEFAULT 0 NOT NULL,
  path VARCHAR NOT NULL, -- relative to storage dir
  format CHARACTER(1) NOT NULL
)
