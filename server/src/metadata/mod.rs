use rocket::fairing::AdHoc;
use rocket::form::Form;
use rocket::response::Debug;
use rocket::serde::json::Json;
use rocket::{Shutdown, State};

use std::sync::Mutex;
use std::time::Duration;

use crate::auth::user::User;

mod db;
mod middleware;
mod models;
mod notification;
mod request;
mod response;
mod schema;

use db::{has_files as db_has_files, insert_new_record, latest_for_path, list as db_list, Db};
use models::{FileRecord, NewFileRecord};

use notification::{ActiveClients, Client};

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit?<uuid>", data = "<commit_payload>")]
async fn commit(
    user: User,
    clients: &State<Mutex<ActiveClients>>,
    db: Db,
    uuid: String,
    commit_payload: Form<request::CommitPayload<'_>>,
) -> Result<Json<response::CommitResultStatus>> {
    let to_be_uploaded = commit_payload.non_local_chunks();

    match to_be_uploaded.is_empty() {
        true => {
            let r = NewFileRecord::from_payload_and_user_id(commit_payload, user.id);

            // Dedup: if the latest record for (user_id, path) already has the
            // same chunk_ids and deleted flag, this commit is a no-op. Return
            // the existing id without inserting or notifying other clients.
            // Guards against buggy / outdated clients that re-commit unchanged
            // files in a loop.
            let dedup_path = r.path.clone();
            let dedup_chunks = r.chunk_ids.clone();
            let dedup_deleted = r.deleted;
            let dedup_user = r.user_id;
            let existing = db
                .run(move |conn| latest_for_path(conn, dedup_user, &dedup_path))
                .await?;

            if let Some(existing) = existing {
                if existing.chunk_ids == dedup_chunks && existing.deleted == dedup_deleted {
                    return Ok(Json(response::CommitResultStatus::Success(existing.id)));
                }
            }

            let id: i32 = db.run(move |conn| insert_new_record(conn, r)).await?;

            clients.lock().unwrap().notify(uuid);

            Ok(Json(response::CommitResultStatus::Success(id)))
        }
        false => {
            let to_be_uploaded_strings: Vec<String> = to_be_uploaded
                .iter()
                .map(|chunk_id| chunk_id.0.to_string())
                .collect();

            Ok(Json(response::CommitResultStatus::NeedChunks(
                to_be_uploaded_strings.join(","),
            )))
        }
    }
}

#[get("/has_files")]
async fn has_files(db: Db, user: User) -> Result<Json<bool>> {
    let result = db.run(move |conn| db_has_files(conn, user.id)).await?;

    Ok(Json(result))
}

// return back array of jid, path, hashes for all jid since requested
#[get("/list?<jid>")]
async fn list(db: Db, user: User, jid: i32) -> Result<Json<Vec<FileRecord>>> {
    let records = db.run(move |conn| db_list(conn, user.id, jid)).await?;

    Ok(Json(records))
}

#[get("/poll?<seconds>&<uuid>")]
async fn poll(
    _user: User,
    clients: &State<Mutex<ActiveClients>>,
    uuid: String,
    seconds: u64,
    shutdown: Shutdown,
) -> Result<()> {
    let notification = {
        let mut data = clients.lock().unwrap();

        let client = Client::new(uuid);

        match data.clients.get(&client) {
            Some(c) => c.notification.clone(),
            None => {
                let notification = client.notification.clone();
                data.clients.insert(client);
                notification
            }
        }
    };

    let timeout = tokio::time::timeout(Duration::from_secs(seconds), notification.notified());

    tokio::select! {
        _ = shutdown => Ok(()),
        _ = timeout => Ok(()),
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Diesel DB Stage", |rocket| async {
        let clients = notification::init();

        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite(
                "Diesel Migrations",
                middleware::run_migrations,
            ))
            .mount("/metadata", routes![commit, has_files, list, poll])
            .manage(clients)
    })
}
