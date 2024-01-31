
use rocket::fairing::AdHoc;
use rocket::response::{status::Created, Debug};
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket};

use rocket_sync_db_pools::database;

use diesel::prelude::*;

use crate::schema::file_records::dsl::*;
use crate::models::*;

#[database("diesel")]
struct Db(diesel::SqliteConnection);

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

#[post("/commit", data = "<file_record>")]
async fn commit(db: Db, mut file_record: Json<FileRecord>) -> Result<Json<FileRecord>> {

    Ok(file_record)
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
            .mount("/metadata", routes![commit])
    })
}
