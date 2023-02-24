use crate::{
    analytics::{Analytics, AnalyticsIden},
    database::DatabaseInstance,
    error::map_eyre_error,
    types::{Time, Uri},
};
use axum::{extract, response::Response, Extension};
use eyre::Context;
use sea_query::{Expr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::{Deserialize, Serialize};
use time::Duration;

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
) -> eyre::Result<Vec<Analytics>> {
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
            for analytics_result in statement
                .query_map(&*values.as_params(), |row| Analytics::try_from(row))
                .wrap_err("Error performing query to obtain `Analytics`")?
            {
                let analytics =
                    analytics_result.wrap_err("Error converting query row into `Analytics`")?;
                data.push(analytics);
            }

            Ok::<_, eyre::Error>(data)
        })
        .await?
}

/// Condenses the data into periods of `window`.
fn condense_data(data: Vec<Analytics>, window: Duration) -> Vec<Analytics> {
    let mut condensed_data = Vec::new();
    let mut i = 0;
    let mut j;
    let mut k;
    while i < data.len() {
        let mut condensed = data[i].clone();

        j = i + 1;
        while j < data.len() && *data[j].time - *condensed.time <= (window / 2) {
            condensed.visits += data[j].visits;
            j += 1;
        }

        if i > 0 {
            k = i - 1;
            while k > 0 && *condensed.time - *data[k].time <= (window / 2) {
                condensed.visits += data[k].visits;
                k -= 1;
            }
        }

        while j < data.len() && *data[j].time - *condensed.time <= window {
            j += 1;
        }

        condensed_data.push(condensed);
        i = j;
    }

    condensed_data
}

fn graph_data(data: Vec<Analytics>) -> eyre::Result<GraphData> {
    if data.is_empty() {
        return Ok(vec![vec![], vec![]]);
    }
    let duration = data.last().expect("not empty").time - data.first().expect("not empty").time;
    let condense_window = duration / 500;
    let data = condense_data(data, condense_window);

    let mut graph_data = Vec::new();
    let (time_data, visits_data): (Vec<u64>, Vec<f64>) = data
        .iter()
        .enumerate()
        .map(|(_i, analytics)| {
            let time: u64 = analytics.time.unix_timestamp().try_into()?;
            Ok::<_, eyre::Error>((time, analytics.visits))
        })
        .fold::<eyre::Result<(Vec<u64>, Vec<f64>)>, _>(
            Ok(Default::default()),
            |acc_result, result| match acc_result {
                Ok(mut acc) => match result {
                    Ok((timestamp, visits)) => {
                        acc.0.push(timestamp);
                        acc.1.push(visits.try_into()?);
                        Ok(acc)
                    }
                    Err(error) => Err(error),
                },
                _ => acc_result,
            },
        )?;

    graph_data.push(
        time_data
            .into_iter()
            .map(minijinja::value::Value::from)
            .collect(),
    );
    graph_data.push(
        visits_data
            .into_iter()
            .map(minijinja::value::Value::from)
            .collect(),
    );

    Ok(graph_data)
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
type GraphData = Vec<Vec<minijinja::value::Value>>;

#[tracing::instrument(skip_all, fields(uri = query.uri.to_string()))]
pub async fn handler(
    Extension(database): Extension<DatabaseInstance>,
    Extension(templates): Extension<TemplatesWithContext>,
    extract::Query(query): extract::Query<Query>,
) -> axum::response::Result<Response> {
    let data = get_analytics_data(&database, query.uri.clone(), query.from, query.to)
        .await
        .and_then(graph_data)
        .map_err(map_eyre_error)?;
    let graph = Graph {
        data,
        uri: query.uri,
    };

    render(&templates.environment, "admin/analytics/graph.html", &graph)
}