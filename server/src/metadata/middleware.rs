use rocket::{Build, Rocket};

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

use super::db::Db;

// TODO should be really in the same folder so we don't forget to add both migrations
#[cfg(feature = "database_sqlite")]
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/metadata/migrations/sqlite");
#[cfg(feature = "database_postgres")]
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/metadata/migrations/postgres");

pub(crate) async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    use diesel_migrations::MigrationHarness;

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
