use rocket::fairing::AdHoc;
use rocket::form::Form;
use rocket::response::Debug;
use rocket::serde::json::Json;
use rocket::{Shutdown, State};

use std::sync::Mutex;
use std::time::Duration;

use crate::auth::User;

use crate::db::{insert_new_record, list as db_list, Db};
use crate::models::{FileRecord, NewFileRecord};

mod middleware;
mod notification;
mod request;
mod response;

use notification::{ActiveClients, Client};

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit?<uuid>", data = "<commit_payload>")]
async fn commit(
    _user: User,
    clients: &State<Mutex<ActiveClients>>,
    db: Db,
    uuid: String,
    commit_payload: Form<request::CommitPayload<'_>>,
) -> Result<Json<response::CommitResultStatus>> {
    let to_be_uploaded = commit_payload.non_local_chunks();

    match to_be_uploaded.is_empty() {
        true => {
            let r: NewFileRecord = commit_payload.into();
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

// return back array of jid, path, hashes for all jid since requested
#[get("/list?<jid>")]
async fn list(db: Db, _user: User, jid: i32) -> Result<Json<Vec<FileRecord>>> {
    let records = db.run(move |conn| db_list(conn, jid)).await?;

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
    AdHoc::on_ignite("Diesel SQLite Stage", |rocket| async {
        let clients = notification::init();

        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite(
                "Diesel Migrations",
                middleware::run_migrations,
            ))
            .mount("/metadata", routes![commit, list, poll])
            .manage(clients)
    })
}
