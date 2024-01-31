
use rocket::fairing::AdHoc;
use rocket::response::{status::Created, Debug};
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket};
use rocket::form::{Form, FromForm};

use rocket_sync_db_pools::database;

use diesel::prelude::*;

use crate::schema::*;
use crate::models::*;

#[database("diesel")]
struct Db(diesel::SqliteConnection);

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;


#[derive(FromForm)]
struct CommitPayload<'r> {
    path: &'r str,
    chunk_ids: Vec<&'r str>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
enum CommitResultStatus {
    Success(i32),
    NeedChunks(Vec<String>)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ListResult {
    jid: i32,
    path: String,
    chunk_ids: Vec<String>
}

// check if all hashes are present
// if any not present return back need more and list of hashes
// if present all insert into db path and chunk hashes and return back a new jid
#[post("/commit", data = "<commit_payload>")]
async fn commit(db: Db, mut commit_payload: Form<CommitPayload<'_>>) -> Result<Json<CommitResultStatus>> {

    Ok(Json(CommitResultStatus::NeedChunks(vec!["sdfsdf".into(), "pfsfd".into()])))
    // Ok(Json(CommitResultStatus::Success(100)))
}

// return back array of jid, path, hashes for all jid since requested
#[get("/list?<jid>")]
async fn list(db: Db, jid: i32) -> Result<Json<Vec<ListResult>>> {
    let r = ListResult {
        jid: 123,
        path: "./tmp/recipe".into(),
        chunk_ids: vec!["hehe".into(), "puk".into()]
    };

    Ok(Json(vec![r]))
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
