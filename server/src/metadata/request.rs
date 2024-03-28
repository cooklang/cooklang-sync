use std::convert::From;

use rocket::form::{Form, FromForm};

use crate::chunk_id::ChunkId;
use crate::models::NewFileRecord;

#[derive(Debug, FromForm)]
pub(crate) struct CommitPayload<'r> {
    path: &'r str,
    deleted: bool,
    chunk_ids: &'r str,
}

impl<'a> CommitPayload<'a> {
    pub(crate) fn non_local_chunks(&self) -> Vec<ChunkId> {
        let desired: Vec<&str> = self.chunk_ids.split(',').collect();

        desired
            .into_iter()
            .map(|c| ChunkId(std::borrow::Cow::Borrowed(c)))
            .filter(|c| !c.is_present())
            .collect()
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
