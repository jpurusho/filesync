use rusqlite_migration::{M, Migrations};

const INIT: &str = r"
CREATE TABLE meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

INSERT INTO meta (key, value) VALUES ('schema_version', '1');
";

#[must_use]
pub fn get_migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(INIT)])
}
