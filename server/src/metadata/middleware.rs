use rocket::{Build, Rocket};

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

use crate::db::Db;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

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
