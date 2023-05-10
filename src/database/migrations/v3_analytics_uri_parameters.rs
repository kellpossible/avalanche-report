use http::Uri;
use rusqlite::ToSql;
use uuid::Uuid;

pub fn run(conn: &rusqlite::Connection) -> eyre::Result<()> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, uri FROM analytics;
        "#,
    )?;

    for analytics in statement.query_map((), |row| {
        let id: Uuid = row.get("id")?;
        let uri_string: String = row.get("uri")?;
        let uri: Uri = uri_string.parse().unwrap();

        Ok((id, uri))
    })? {
        let analytics = analytics?;
        let new_uri: String = analytics.1.path().to_owned();
        conn.execute(
            r#"
            UPDATE analytics SET uri = ? WHERE id = ?;
            "#,
            [new_uri.to_sql()?, analytics.0.to_sql()?],
        )?;
    }
    Ok(())
}
