use std::convert::From;

use rocket::form::{Form, FromForm};

use crate::chunk_id::ChunkId;
use crate::models::NewFileRecord;

#[derive(Debug, FromForm)]
pub(crate) struct CommitPayload<'r> {
    path: &'r str,
    deleted: bool,
    chunk_ids: &'r str,
    format: &'r str,
}

impl From<Form<CommitPayload<'_>>> for NewFileRecord {
    fn from(payload: Form<CommitPayload<'_>>) -> Self {
        NewFileRecord {
            path: payload.path.into(),
            deleted: payload.deleted,
            chunk_ids: payload.chunk_ids.into(),
            format: payload.format.into(),
        }
    }
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
