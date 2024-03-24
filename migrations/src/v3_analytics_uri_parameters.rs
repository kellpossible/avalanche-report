use http::Uri;
use sqlx::Row;
use uuid::Uuid;

pub async fn run(conn: sqlx::SqlitePool) -> eyre::Result<()> {
    for (id, uri) in sqlx::query(
        r#"
        SELECT id, uri FROM analytics;
        "#,
    )
    .fetch_all(&conn)
    .await?
    .into_iter()
    .map(|row| {
        let id: Uuid = row.get("id");
        let uri_string: String = row.get("uri");
        let uri: Uri = uri_string.parse().unwrap();
        (id, uri)
    }) {
        let new_uri: String = uri.path().to_owned();
        sqlx::query(
            r#"
                UPDATE analytics SET uri = ? WHERE id = ?;
                "#,
        )
        .bind(new_uri)
        .bind(id)
        .execute(&conn)
        .await?;
    }
    Ok(())
}
