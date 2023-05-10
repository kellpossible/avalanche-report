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

pub fn run(conn: &rusqlite::Connection) -> eyre::Result<()> {
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
