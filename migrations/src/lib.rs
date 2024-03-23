use std::pin::Pin;

use base64::Engine;
use eyre::Context;
use nonzero_ext::nonzero;
use sha2::{Digest, Sha256};
use sqlx::Row;
use time::format_description::well_known::iso8601::TimePrecision;
use time::format_description::well_known::{iso8601, Iso8601};

pub const DATETIME_CONFIG: iso8601::EncodedConfig = iso8601::Config::DEFAULT
    .set_time_precision(TimePrecision::Second {
        decimal_digits: Some(nonzero!(3u8)),
    })
    .encode();
pub const DATETIME_FORMAT: Iso8601<DATETIME_CONFIG> = Iso8601;

mod v2_analytics_time_format;
mod v3_analytics_uri_parameters;

enum MigrationKind {
    Sql(&'static str),
    Rust(
        Box<
            dyn Fn(
                sqlx::SqlitePool,
            ) -> Pin<Box<dyn std::future::Future<Output = eyre::Result<()>>>>,
        >,
    ),
}

impl MigrationKind {
    pub fn rust<
        FUT: std::future::Future<Output = eyre::Result<()>> + 'static,
        F: Fn(sqlx::SqlitePool) -> FUT + 'static,
    >(
        migration: F,
    ) -> Self {
        Self::Rust(Box::new(move |conn| Box::pin(migration(conn))))
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
    async fn run(&self, conn: &sqlx::SqlitePool) -> eyre::Result<()> {
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
                f(conn.clone())
                    .await
                    .wrap_err_with(|| format!("Error executing migration function {f:p}"))?;
            }
        }
        tracing::debug!("Migration complete!");

        record_migration(conn, self)
            .await
            .wrap_err("Error while recording migration")?;

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

async fn current_migration(conn: &sqlx::SqlitePool) -> eyre::Result<Option<u32>> {
    let schema_history_table_name: Option<String> = Option::transpose(
        sqlx::query(r#"SELECT "name" FROM pragma_table_info("schema_history") LIMIT 1;"#)
            .fetch_optional(conn)
            .await?
            .map(|row| row.try_get("name")),
    )?;

    if schema_history_table_name.is_none() {
        return Ok(None);
    }
    let version: Option<u32> = Option::transpose(
        sqlx::query(
            r#"
            SELECT version FROM schema_history
            WHERE version = (SELECT MAX(version) from schema_history);
            "#,
        )
        .fetch_optional(conn)
        .await?
        .map(|row| row.try_get("version")),
    )?;
    Ok(version)
}

async fn record_migration(conn: &sqlx::SqlitePool, migration: &Migration) -> eyre::Result<()> {
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
    .bind(time::OffsetDateTime::now_utc().format(&DATETIME_FORMAT)?)
    .bind(checksum)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn run(conn: &sqlx::SqlitePool) -> eyre::Result<()> {
    let migrations: Vec<Migration> = list_migrations();

    async fn run_migrations(
        mut current_migration_index: usize,
        conn: &sqlx::SqlitePool,
        migrations: &[Migration],
    ) -> eyre::Result<()> {
        while let Some(migration) = migrations.get(current_migration_index + 1) {
            migration.run(conn).await?;
            current_migration_index += 1;
        }

        Ok(())
    }
    if let Some(current_migration) = current_migration(conn)
        .await
        .wrap_err("Error obtaining current migration")?
    {
        run_migrations(current_migration.try_into()?, conn, &migrations).await?
    } else {
        migrations[0].run(conn).await?;
        run_migrations(0, conn, &migrations).await?
    }

    tracing::info!("All migrations completed successfully.");

    Ok(())
}
