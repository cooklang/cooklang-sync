-- Per-namespace download watermark. Separated from file_records so it can
-- be advanced atomically after a full download batch completes, instead of
-- being derived from max(file_records.jid) which drifts forward whenever
-- any single file is saved. See fix/download-watermark for context.
CREATE TABLE sync_state (
  namespace_id INTEGER PRIMARY KEY NOT NULL,
  download_watermark INTEGER NOT NULL DEFAULT 0
);
