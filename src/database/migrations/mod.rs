use std::pin::Pin;

use base64::Engine;
use eyre::Context;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::Executor;

use super::DATETIME_FORMAT;

mod v2_analytics_time_format;
mod v3_analytics_uri_parameters;

enum MigrationKind {
    Sql(&'static str),
    Rust(
        fn(&sqlx::SqliteConnection) -> Pin<Box<dyn std::future::Future<Output = eyre::Result<()>>>>,
    ),
}

impl MigrationKind {
    pub fn rust<
        FUT: std::future::Future<Output = eyre::Result<()>>,
        F: Fn(&sqlx::SqliteConnection) -> FUT,
    >(
        migration: F,
    ) -> Self {
        Self::Rust(move |conn| Box::pin(migration(conn)))
    }
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
    async fn run(&self, conn: &sqlx::SqliteConnection) -> eyre::Result<()> {
        tracing::info!("Running migration {}", self.name);
        match &self.kind {
            MigrationKind::Sql(sql) => {
                tracing::debug!("Performing SQL Migration: \n{0}", blue(sql));
                sqlx::raw_sql(sql)
                    .execute(conn)
                    .await
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
            kind: MigrationKind::rust(v2_analytics_time_format::run),
        },
        Migration {
            version: 3,
            name: "analytics_uri_parameters",
            kind: MigrationKind::rust(v3_analytics_uri_parameters::run),
        },
        Migration {
            version: 4,
            name: "forecast_files",
            kind: MigrationKind::Sql(include_str!("v4_forecast_files.sql")),
        },
        Migration {
            version: 5,
            name: "forecast_json_cache",
            kind: MigrationKind::Sql(include_str!("v5_forecast_json_cache.sql")),
        },
        Migration {
            version: 6,
            name: "current_weather_cache",
            kind: MigrationKind::Sql(include_str!("v6_current_weather_cache.sql")),
        },
        Migration {
            version: 7,
            name: "forecast_areas",
            kind: MigrationKind::Sql(include_str!("v7_forecast_areas.sql")),
        },
    ]
}

async fn current_migration(conn: &sqlx::SqliteConnection) -> eyre::Result<Option<u32>> {
    let schema_history_table_name: Option<String> =
        sqlx::query(r#"SELECT "name" FROM pragma_table_info("schema_history") LIMIT 1;"#)
            .fetch_optional(conn)
            .await?;

    if schema_history_table_name.is_none() {
        return Ok(None);
    }
    let version: Option<u32> = sqlx::query(
        r#"
            SELECT version FROM schema_history
            WHERE version = (SELECT MAX(version) from schema_history);
            "#,
    )
    .fetch_optional(conn)
    .await?;
    Ok(version)
}

async fn record_migration(
    conn: &sqlx::SqliteConnection,
    migration: &Migration,
) -> eyre::Result<()> {
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
    sqlx::query(
        r#"
        INSERT INTO schema_history (version, name, applied_on, checksum)
        VALUES(?, ?, ?, ?)
        "#,
    )
    .bind(version)
    .bind(migration.name)
    .bind(time::OffsetDateTime::now_utc().format(&DATETIME_FORMAT))
    .bind(checksum)
    .execute(conn)
    .await?;
    Ok(())
}

pub fn run(conn: &sqlx::SqliteConnection) -> eyre::Result<()> {
    let migrations: Vec<Migration> = list_migrations();

    fn run_migrations(
        mut current_migration_index: usize,
        conn: &sqlx::SqliteConnection,
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
