use rocket::fairing::AdHoc;
use rocket::form::{Form, FromForm};
use rocket::response::Debug;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket, State};

use rocket_sync_db_pools::database;
use diesel::prelude::*;

use async_notify::Notify;
use std::sync::{Arc, Mutex};
use std::time::{Duration};

use crate::chunk_id::ChunkId;
use crate::models::*;
use crate::schema::*;

#[database("diesel")]
struct Db(diesel::SqliteConnection);

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

#[derive(Debug)]
#[derive(FromForm)]
struct CommitPayload<'r> {
    path: &'r str,
    deleted: bool,
    chunk_ids: &'r str,
    format: &'r str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
enum CommitResultStatus {
    Success(i32),
    NeedChunks(String),
}


struct ActiveUsers {
    notification: Arc<Notify>
}

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit", data = "<commit_payload>")]
async fn commit(
    users: &State<Mutex<ActiveUsers>>,
    db: Db,
    commit_payload: Form<CommitPayload<'_>>,
) -> Result<Json<CommitResultStatus>> {
    debug_!("commit payload {:?}", commit_payload);
    let desired: Vec<&str> = commit_payload.chunk_ids.split(',').collect();

    let notification = users.lock().unwrap().notification.clone();

    let to_be_uploaded: Vec<ChunkId> = desired
        .into_iter()
        .map(|c| ChunkId(std::borrow::Cow::Borrowed(c)))
        .filter(|c| !c.is_present())
        .collect();

    if to_be_uploaded.is_empty() {
        let r = NewFileRecord {
            path: commit_payload.path.into(),
            deleted: commit_payload.deleted,
            chunk_ids: commit_payload.chunk_ids.into(),
            format: commit_payload.format.into(),
        };
        let id: i32 = db
            .run(move |conn| {
                diesel::insert_into(file_records::table)
                    .values(r)
                    .returning(file_records::id)
                    .get_result(conn)
            })
            .await?;

        notification.notify();

        Ok(Json(CommitResultStatus::Success(id)))
    } else {
        let to_be_uploaded_strings: Vec<String> = to_be_uploaded
            .iter()
            .map(|chunk_id| chunk_id.0.to_string())
            .collect();

        notification.notify();

        Ok(Json(CommitResultStatus::NeedChunks(
            to_be_uploaded_strings.join(","),
        )))
    }
}

// return back array of jid, path, hashes for all jid since requested
#[get("/list?<jid>")]
async fn list(db: Db, jid: i32) -> Result<Json<Vec<FileRecord>>> {
    debug_!("list after {:?}", jid);
    let records: Vec<FileRecord> = db
        .run(move |conn| {
            file_records::table
                .filter(file_records::id.gt(jid))
                .select(FileRecord::as_select())
                .load(conn)
        })
        .await?;

    Ok(Json(records))
}

#[get("/poll?<seconds>")]
async fn poll(users: &State<Mutex<ActiveUsers>>, seconds: u64) -> String {
    let notification = users.lock().unwrap().notification.clone();

    match tokio::time::timeout(Duration::from_secs(seconds), notification.notified()).await {
        Ok(_) => {
            "Done".to_string()
        },
        Err(_) => {
            "Timeout".to_string()
        }
    }
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    Db::get_one(&rocket)
        .await
        .expect("database connection")
        .run(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .expect("diesel migrations");
        })
        .await;

    rocket
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Diesel SQLite Stage", |rocket| async {
        let users = Mutex::new(ActiveUsers { notification: Arc::new(Notify::new()) });

        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite("Diesel Migrations", run_migrations))
            .mount("/metadata", routes![commit, list, poll])
            .manage(users)
    })
}
