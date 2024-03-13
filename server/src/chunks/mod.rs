use multer::Multipart;
use rocket::data::{Data, Limits};
use rocket::fairing::AdHoc;
use rocket::response::content::RawText;
use rocket::tokio::fs::{self, create_dir_all, File};
use rocket::tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use crate::auth::user::User;
use crate::chunk_id::ChunkId;

mod request;

use crate::chunks::request::RawContentType;

const EMPTY_CHUNK_ID: ChunkId = ChunkId(std::borrow::Cow::Borrowed(""));

#[post("/", format = "multipart/form-data", data = "<upload>")]
async fn upload_chunks(
    _user: User,
    content_type: RawContentType<'_>,
    limits: &Limits,
    upload: Data<'_>,
) -> std::io::Result<()> {
    let boundary = multer::parse_boundary(content_type.0).unwrap();
    let upload_stream = upload.open(limits.get("data-form").unwrap());
    let mut multipart = Multipart::new(ReaderStream::new(upload_stream), boundary);

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let field_name = field.name().unwrap();
        let chunk_id = ChunkId::from(field_name);

        if chunk_id == EMPTY_CHUNK_ID {
            continue;
        }

        let full_path = chunk_id.file_path();

        if let Some(parent) = full_path.parent() {
            create_dir_all(parent).await?;
        }

        let mut file = File::create(full_path.clone()).await?;

        while let Some(chunk) = match field.chunk().await {
            Ok(v) => v,
            Err(_e) => {
                fs::remove_file(&full_path).await.ok();

                // TODO
                panic!("Error reading chunk");
            }
        } {
            file.write_all(&chunk).await.map_err(|_| {
                std::fs::remove_file(&full_path).ok();
            });
        }
    }

    Ok(())
}

/// Downloads chunk from a storage
// TODO batch download
#[get("/<id>")]
async fn retrieve(_user: User, id: ChunkId<'_>) -> Option<RawText<File>> {
    if id == EMPTY_CHUNK_ID {
        None
    } else {
        File::open(id.file_path()).await.map(RawText).ok()
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Chunk Server Stage", |rocket| async {
        rocket.mount("/chunks", routes![upload_chunks, retrieve])
    })
}
