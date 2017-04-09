use diesel::prelude::*;
use diesel::pg::PgConnection;

// TODO(loyd): use UNIX socket in production.
const DATABASE_URL: &str = "postgres://localhost/chokurei";

pub fn establish_connection() -> ConnectionResult<PgConnection> {
    PgConnection::establish(DATABASE_URL)
}

infer_schema!("postgres://localhost/chokurei");
