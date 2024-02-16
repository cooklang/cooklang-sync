use rocket::fairing::AdHoc;
use rocket::form::{Form, FromForm};
use rocket::response::Debug;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket, Shutdown, State};
use rocket_sync_db_pools::database;

use diesel::dsl::{max, sql};
use diesel::prelude::*;

use async_notify::Notify;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::chunk_id::ChunkId;
use crate::models::*;
use crate::schema::*;
use crate::auth::User;

#[database("diesel")]
struct Db(diesel::SqliteConnection);

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

#[derive(Debug, FromForm)]
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

struct Client {
    uuid: String,
    notification: Arc<Notify>,
}

impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl Eq for Client {}

impl Hash for Client {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

struct ActiveClients {
    clients: HashSet<Client>,
}

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit?<uuid>", data = "<commit_payload>")]
async fn commit(
    clients: &State<Mutex<ActiveClients>>,
    db: Db,
    _user: User,
    uuid: String,
    commit_payload: Form<CommitPayload<'_>>,
) -> Result<Json<CommitResultStatus>> {
    debug_!("commit payload {:?}", commit_payload);
    let desired: Vec<&str> = commit_payload.chunk_ids.split(',').collect();

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

        let notifications: Vec<Arc<Notify>> = clients
            .lock()
            .unwrap()
            .clients
            .iter()
            .filter(|client| client.uuid != uuid)
            .map(|client| Arc::clone(&client.notification))
            .collect();

        for notification in notifications {
            notification.notify();
        }

        Ok(Json(CommitResultStatus::Success(id)))
    } else {
        let to_be_uploaded_strings: Vec<String> = to_be_uploaded
            .iter()
            .map(|chunk_id| chunk_id.0.to_string())
            .collect();

        Ok(Json(CommitResultStatus::NeedChunks(
            to_be_uploaded_strings.join(","),
        )))
    }
}

// return back array of jid, path, hashes for all jid since requested
#[get("/list?<jid>")]
async fn list(db: Db, _user: User, jid: i32) -> Result<Json<Vec<FileRecord>>> {
    let records: Vec<FileRecord> = db
        .run(move |conn| {
            // Consider only latest record for the same path.
            let subquery = file_records::table
                .group_by(file_records::path)
                .select(max(file_records::id))
                .into_boxed()
                .select(sql::<diesel::sql_types::Integer>("max(id)"));

            file_records::table
                .filter(file_records::id.gt(jid))
                .filter(file_records::id.eq_any(subquery))
                .select(FileRecord::as_select())
                .load(conn)
        })
        .await?;

    Ok(Json(records))
}

#[get("/poll?<seconds>&<uuid>")]
async fn poll(
    clients: &State<Mutex<ActiveClients>>,
    uuid: String,
    _user: User,
    seconds: u64,
    shutdown: Shutdown,
) -> String {
    let notification = {
        let mut data = clients.lock().unwrap();

        let client = Client {
            uuid: uuid.clone(),
            notification: Arc::new(Notify::new()),
        };

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
        _ = shutdown => {
            "Server shutting down".to_string()
        }
        result = timeout => {
            match result {
                Ok(_) => "Done".to_string(),
                Err(_) => "Timeout".to_string(),
            }
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
        let clients = Mutex::new(ActiveClients {
            clients: HashSet::new(),
        });

        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite("Diesel Migrations", run_migrations))
            .mount("/metadata", routes![commit, list, poll])
            .manage(clients)
    })
}
