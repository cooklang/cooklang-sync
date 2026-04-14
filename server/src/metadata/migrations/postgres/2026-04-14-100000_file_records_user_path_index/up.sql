-- Covering index for:
--   * /metadata/commit dedup lookup: latest row for (user_id, path)
--   * /metadata/list subquery: max(id) grouped by path, filtered by user_id
-- Without this, a user with many rows for one path makes every commit do a
-- full table scan.
CREATE INDEX IF NOT EXISTS idx_file_records_user_path_id
    ON file_records (user_id, path, id DESC);
