use std::convert::From;

use rocket::form::{Form, FromForm};

use super::models::NewFileRecord;
use crate::chunk_id::ChunkId;

#[derive(Debug, FromForm)]
pub(crate) struct CommitPayload<'r> {
    path: &'r str,
    deleted: bool,
    chunk_ids: &'r str,
}

impl<'a> CommitPayload<'a> {
    /// Returns chunks referenced by this commit that are not stored yet, or
    /// `None` if any of the chunk ids is invalid.
    pub(crate) fn non_local_chunks(&self) -> Option<Vec<ChunkId<'_>>> {
        let mut missing = Vec::new();

        for c in self.chunk_ids.split(',') {
            let id = ChunkId::parse(c)?;
            if !id.is_present() {
                missing.push(id);
            }
        }

        Some(missing)
    }
}

impl NewFileRecord {
    pub(crate) fn from_payload_and_user_id(payload: Form<CommitPayload<'_>>, user_id: i32) -> Self {
        NewFileRecord {
            path: payload.path.into(),
            deleted: payload.deleted,
            chunk_ids: payload.chunk_ids.into(),
            user_id,
        }
    }
}
