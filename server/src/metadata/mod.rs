use rocket::fairing::AdHoc;
use rocket::form::{Form, FromForm};
use rocket::response::Debug;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket};

use rocket_sync_db_pools::database;

use diesel::prelude::*;

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
    chunk_ids: &'r str,
    format: &'r str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
enum CommitResultStatus {
    Success(i32),
    NeedChunks(String),
}

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit", data = "<commit_payload>")]
async fn commit(
    db: Db,
    commit_payload: Form<CommitPayload<'_>>,
) -> Result<Json<CommitResultStatus>> {
    let desired: Vec<&str> = commit_payload.chunk_ids.split(',').collect();

    let to_be_uploaded: Vec<ChunkId> = desired
        .into_iter()
        .map(|c| ChunkId(std::borrow::Cow::Borrowed(c)))
        .filter(|c| !c.is_present())
        .collect();

    if to_be_uploaded.is_empty() {
        let r = NewFileRecord {
            path: commit_payload.path.into(),
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
async fn list(db: Db, jid: i32) -> Result<Json<Vec<FileRecord>>> {
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
        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite("Diesel Migrations", run_migrations))
            .mount("/metadata", routes![commit, list])
    })
}
