use base64::Engine;
use eyre::Context;
use rusqlite::{OptionalExtension, ToSql};
use sha2::{Digest, Sha256};

use super::DATETIME_FORMAT;

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

mod migrations {
    use http::Uri;
    use nonzero_ext::nonzero;
    use rusqlite::ToSql;
    use time::{
        format_description::well_known::{
            iso8601::{self, TimePrecision},
            Iso8601,
        },
        macros::format_description,
        OffsetDateTime,
    };
    use uuid::Uuid;

    const DATETIME_CONFIG: iso8601::EncodedConfig = iso8601::Config::DEFAULT
        .set_time_precision(TimePrecision::Second {
            decimal_digits: Some(nonzero!(3u8)),
        })
        .encode();
    const DATETIME_FORMAT: iso8601::Iso8601<DATETIME_CONFIG> = Iso8601;

    pub fn v2_analytics_time_format(conn: &rusqlite::Connection) -> eyre::Result<()> {
        #[allow(unused)]
        struct Analytics {
            pub id: uuid::Uuid,
            pub uri: Uri,
            pub visits: u64,
            pub time: time::OffsetDateTime,
        }

        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE analytics
            RENAME COLUMN time TO old_time;
            ALTER TABLE analytics
            ADD COLUMN time TEXT;
            COMMIT;
        "#,
        )?;

        let mut statement = conn.prepare(
            r#"
            SELECT * FROM analytics;
            "#,
        )?;

        let original_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:6] [offset_hour sign:mandatory]:[offset_minute]");
        // let new_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");
        let new_format = DATETIME_FORMAT;
        for analytics in statement.query_map((), |row| {
            let id: Uuid = row.get("id")?;
            let uri_string: String = row.get("uri")?;
            let uri = uri_string.parse().unwrap();
            let visits = row.get("visits")?;
            let time_string: String = row.get("old_time")?;
            let time = OffsetDateTime::parse(&time_string, &original_format).unwrap();
            Ok(Analytics {
                id,
                uri,
                visits,
                time,
            })
        })? {
            let analytics = analytics?;
            let new_time: String = analytics.time.format(&new_format)?;
            conn.execute(
                r#"
                UPDATE analytics SET time = ? WHERE id = ?;
                "#,
                [new_time.to_sql()?, analytics.id.to_sql()?],
            )?;
        }
        conn.execute_batch(
            r#"
            ALTER TABLE analytics
            DROP COLUMN old_time;
        "#,
        )?;
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
            kind: MigrationKind::Rust(migrations::v2_analytics_time_format),
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
    let name = migration.name.clone();
    conn.execute(
        r#"
        INSERT INTO schema_history (version, name, applied_on, checksum)
        VALUES(?, ?, ?, ?)
        "#,
        [
            version.to_sql()?,
            name.to_sql()?,
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
