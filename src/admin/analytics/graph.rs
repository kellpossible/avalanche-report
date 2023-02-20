use crate::{
    analytics::{Analytics, AnalyticsIden},
    database::DatabaseInstance,
    error::map_eyre_error,
    serde::string,
    types::{Time, Uri},
};
use axum::{extract, response::Response, Extension};
use eyre::Context;
use sea_query::{Expr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::{Deserialize, Serialize};

use crate::templates::{render, TemplatesWithContext};

#[derive(Deserialize)]
pub struct Query {
    uri: Uri,
    from: Option<Time>,
    to: Option<Time>,
}

async fn get_analytics_data(
    database: &DatabaseInstance,
    uri: Uri,
    from: Option<Time>,
    to: Option<Time>,
) -> eyre::Result<GraphData> {
    database
        .interact(move |conn| {
            let from: Option<sea_query::Value> =
                Option::transpose(from.map(|from| from.try_into()))?;
            let to: Option<sea_query::Value> = Option::transpose(to.map(|to| to.try_into()))?;
            let (sql, values) = sea_query::Query::select()
                .columns(Analytics::COLUMNS)
                .from(Analytics::TABLE)
                .and_where(Expr::col(AnalyticsIden::Uri).eq(uri.to_string()))
                .and_where_option(from.map(|from| Expr::col(AnalyticsIden::Time).gte(from)))
                .and_where_option(to.map(|to| Expr::col(AnalyticsIden::Time).lte(to)))
                .build_rusqlite(SqliteQueryBuilder);

            let mut statement = conn.prepare_cached(&sql)?;
            let mut data = Vec::new();
            data.push(Vec::new());
            data.push(Vec::new());

            for analytics_result in statement
                .query_map(&*values.as_params(), |row| Analytics::try_from(row))
                .wrap_err("Error performing query to obtain `Analytics`")?
            {
                let analytics =
                    analytics_result.wrap_err("Error converting query row into `Analytics`")?;
                data[0].push((analytics.time.unix_timestamp()).try_into()?);
                data[1].push(analytics.visits);
            }

            Ok::<_, eyre::Error>(data)
        })
        .await?
}

#[derive(Serialize)]
struct Graph {
    data: GraphData,
    uri: Uri,
}

///
/// `timestamp` is the unix timestamp in seconds.
/// ```json
/// [
///     [timestamp, timestamp, ...],
///     [visits, visits, ...]
/// ]
/// ```
type GraphData = Vec<Vec<u64>>;

#[tracing::instrument(skip_all, fields(uri = query.uri.to_string()))]
pub async fn handler(
    Extension(database): Extension<DatabaseInstance>,
    Extension(templates): Extension<TemplatesWithContext>,
    extract::Query(query): extract::Query<Query>,
) -> axum::response::Result<Response> {
    let data = get_analytics_data(&database, query.uri.clone(), query.from, query.to)
        .await
        .map_err(map_eyre_error)?;
    let graph = Graph {
        data,
        uri: query.uri,
    };

    render(&templates.environment, "admin/analytics/graph.html", &graph)
}
