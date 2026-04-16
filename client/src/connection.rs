use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use crate::errors::SyncError;

pub type Connection = PooledConnection<ConnectionManager<SqliteConnection>>;
pub type ConnectionPool = Pool<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub fn get_connection_pool(db_path: &str) -> Result<ConnectionPool, SyncError> {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);

    let pool = match Pool::builder().test_on_check_out(true).build(manager) {
        Ok(p) => p,
        Err(e) => return Err(SyncError::ConnectionInitError(e.to_string())),
    };

    let conn = &mut get_connection(&pool)?;

    if let Err(e) = conn.run_pending_migrations(MIGRATIONS) {
        return Err(SyncError::ConnectionInitError(e.to_string()));
    };

    Ok(pool)
}

pub fn get_connection(pool: &ConnectionPool) -> Result<Connection, SyncError> {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => return Err(SyncError::ConnectionInitError(e.to_string())),
    };

    Ok(conn)
}
