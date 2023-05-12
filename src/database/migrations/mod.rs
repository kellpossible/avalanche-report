use base64::Engine;
use eyre::Context;
use rusqlite::{OptionalExtension, ToSql};
use sha2::{Digest, Sha256};

use super::DATETIME_FORMAT;

mod v2_analytics_time_format;
mod v3_analytics_uri_parameters;

enum MigrationKind {
    Sql(&'static str),
    Rust(fn(&rusqlite::Connection) -> eyre::Result<()>),
}

struct Migration {
    version: u32,
    name: &'static str,
    kind: MigrationKind,
}

/// Formats string as blue using ANSI terminal escape codes.
fn blue(string: &str) -> String {
    format!("\x1b[34m{string}\x1b[0m")
}

impl Migration {
    #[tracing::instrument(skip_all, fields(version = self.version))]
    fn run(&self, conn: &rusqlite::Connection) -> eyre::Result<()> {
        tracing::info!("Running migration {}", self.name);
        match &self.kind {
            MigrationKind::Sql(sql) => {
                tracing::debug!("Performing SQL Migration: \n{0}", blue(sql));
                conn.execute_batch(sql)
                    .wrap_err("Error executing migration's SQL query")?;
            }
            MigrationKind::Rust(f) => {
                tracing::debug!("Performing rust function migration. {f:p}");
                f(conn).wrap_err_with(|| format!("Error executing migration function {f:p}"))?;
            }
        }
        tracing::debug!("Migration complete!");

        record_migration(conn, self).wrap_err("Error while recording migration")?;

        Ok(())
    }
}

fn list_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 0,
            name: "schema_history",
            kind: MigrationKind::Sql(include_str!("v0_schema_history.sql")),
        },
        Migration {
            version: 1,
            name: "analytics",
            kind: MigrationKind::Sql(include_str!("v1_analytics.sql")),
        },
        Migration {
            version: 2,
            name: "analytics_time_format",
            kind: MigrationKind::Rust(v2_analytics_time_format::run),
        },
        Migration {
            version: 3,
            name: "analytics_uri_parameters",
            kind: MigrationKind::Rust(v3_analytics_uri_parameters::run),
        },
        Migration {
            version: 4,
            name: "forecast_files",
            kind: MigrationKind::Sql(include_str!("v4_forecast_files.sql")),
        },
    ]
}

fn current_migration(conn: &rusqlite::Connection) -> eyre::Result<Option<u32>> {
    let schema_history_table_name: Option<String> = conn
        .query_row(
            r#"SELECT "name" FROM pragma_table_info("schema_history") LIMIT 1;"#,
            (),
            |row| row.get("name"),
        )
        .optional()?;
    if schema_history_table_name.is_none() {
        return Ok(None);
    }
    let version: Option<u32> = conn
        .query_row(
            r#"
            SELECT version FROM schema_history
            WHERE version = (SELECT MAX(version) from schema_history);
            "#,
            (),
            |row| {
                let version: u32 = row.get("version")?;
                Ok(version)
            },
        )
        .optional()?;
    Ok(version)
}

fn record_migration(conn: &rusqlite::Connection, migration: &Migration) -> eyre::Result<()> {
    let checksum: Option<String> = match migration.kind {
        MigrationKind::Sql(sql) => {
            let mut hasher = Sha256::new();
            hasher.update(sql);
            let result = hasher.finalize();
            let engine = base64::engine::general_purpose::STANDARD_NO_PAD;
            Some(engine.encode(result))
        }
        _ => None,
    };
    let version = migration.version;
    conn.execute(
        r#"
        INSERT INTO schema_history (version, name, applied_on, checksum)
        VALUES(?, ?, ?, ?)
        "#,
        [
            version.to_sql()?,
            migration.name.to_sql()?,
            time::OffsetDateTime::now_utc()
                .format(&DATETIME_FORMAT)?
                .to_sql()?,
            checksum.to_sql()?,
        ],
    )?;
    Ok(())
}

pub fn run(conn: &rusqlite::Connection) -> eyre::Result<()> {
    let migrations: Vec<Migration> = list_migrations();

    fn run_migrations(
        mut current_migration_index: usize,
        conn: &rusqlite::Connection,
        migrations: &[Migration],
    ) -> eyre::Result<()> {
        while let Some(migration) = migrations.get(current_migration_index + 1) {
            migration.run(conn)?;
            current_migration_index += 1;
        }

        Ok(())
    }
    if let Some(current_migration) =
        current_migration(conn).wrap_err("Error obtaining current migration")?
    {
        run_migrations(current_migration.try_into()?, conn, &migrations)?
    } else {
        migrations[0].run(conn)?;
        run_migrations(0, conn, &migrations)?
    }

    tracing::info!("All migrations completed successfully.");

    Ok(())
}
