use http::Uri;
use nonzero_ext::nonzero;
use sqlx::{Executor, Row};
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

pub async fn run(conn: sqlx::SqlitePool) -> eyre::Result<()> {
    #[allow(unused)]
    struct Analytics {
        pub id: uuid::Uuid,
        pub uri: Uri,
        pub visits: u32,
        pub time: time::OffsetDateTime,
    }

    sqlx::raw_sql(
        r#"
        BEGIN;
        ALTER TABLE analytics
        RENAME COLUMN time TO old_time;
        ALTER TABLE analytics
        ADD COLUMN time TEXT;
        COMMIT;
    "#,
    )
    .execute(&conn)
    .await?;

    let original_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:6] [offset_hour sign:mandatory]:[offset_minute]");
    // let new_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");
    let new_format = DATETIME_FORMAT;

    for analytics in sqlx::query("SELECT * FROM analytics")
        .fetch_all(&conn)
        .await?
        .into_iter()
        .map(|row| {
            let id: Uuid = row.get("id");
            let uri_string: String = row.get("uri");
            let uri = uri_string.parse().unwrap();
            let visits = row.get("visits");
            let time_string: String = row.get("old_time");
            let time = OffsetDateTime::parse(&time_string, &original_format).unwrap();
            Analytics {
                id,
                uri,
                visits,
                time,
            }
        })
    {
        let new_time: String = analytics.time.format(&new_format)?;
        sqlx::query(
            r#"
            UPDATE analytics SET time = ? WHERE id = ?;
            "#,
        )
        .bind(new_time)
        .bind(analytics.id)
        .execute(&conn)
        .await?;
    }

    conn.execute(
        r#"
        ALTER TABLE analytics
        DROP COLUMN old_time;
    "#,
    )
    .await?;
    Ok(())
}
