use crate::{
    analytics::{Analytics, AnalyticsIden},
    database::DatabaseInstance,
    types::Time,
};
use eyre::Context;
use sea_query::{Expr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::{Deserialize, Serialize};
use time::Duration;

#[derive(Clone, Copy)]
struct AnalyticsData {
    /// Time in seconds since epoch.
    time: i64,
    visits: u32,
}

async fn get_analytics_data(
    database: &DatabaseInstance,
    options: Options,
) -> eyre::Result<Vec<AnalyticsData>> {
    database
        .interact(move |conn| {
            let from: Option<sea_query::Value> =
                Option::transpose(options.from.map(|from| from.try_into()))?;
            let to: Option<sea_query::Value> =
                Option::transpose(options.to.map(|to| to.try_into()))?;
            let mut query = sea_query::Query::select();
            query
                .columns(Analytics::COLUMNS)
                .from(Analytics::TABLE)
                .and_where_option(from.map(|from| Expr::col(AnalyticsIden::Time).gte(from)))
                .and_where_option(to.map(|to| Expr::col(AnalyticsIden::Time).lte(to)));

            if let Some(uri_filter) = options.uri_filter {
                query.and_where(Expr::cust_with_values(
                    &format!("{} GLOB ?", AnalyticsIden::Uri.as_ref()),
                    [uri_filter],
                ));
            }

            let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);

            let mut statement = conn.prepare_cached(&sql)?;

            let mut data = Vec::new();
            for analytics_result in statement
                .query_map(&*values.as_params(), |row| Analytics::try_from(row))
                .wrap_err("Error performing query to obtain `Analytics`")?
            {
                let analytics =
                    analytics_result.wrap_err("Error converting query row into `Analytics`")?;
                let analytics_data = AnalyticsData {
                    time: analytics.time.unix_timestamp(),
                    visits: analytics.visits,
                };
                data.push(analytics_data);
            }

            Ok::<_, eyre::Error>(data)
        })
        .await?
}

/// Condenses the data into periods of `window_seconds`.
fn condense_data(data: Vec<AnalyticsData>, window_seconds: i64) -> Vec<AnalyticsData> {
    let mut condensed_data = Vec::new();
    let mut i = 0;
    let mut j;
    let mut k;
    while i < data.len() {
        let mut condensed = data[i].clone();

        j = i + 1;
        while j < data.len() && data[j].time - condensed.time <= (window_seconds / 2) {
            condensed.visits += data[j].visits;
            j += 1;
        }

        if i > 0 {
            k = i - 1;
            while k > 0 && condensed.time - data[k].time <= (window_seconds / 2) {
                condensed.visits += data[k].visits;
                k -= 1;
            }
        }

        while j < data.len() && data[j].time - condensed.time <= window_seconds {
            j += 1;
        }

        condensed_data.push(condensed);
        i = j;
    }

    condensed_data
}

fn graph_data(data: Vec<AnalyticsData>) -> eyre::Result<GraphData> {
    if data.is_empty() {
        return Ok(vec![vec![], vec![]]);
    }
    let duration = data.last().expect("not empty").time - data.first().expect("not empty").time;
    let condense_window = duration / 500;
    let data = condense_data(data, condense_window);

    let mut graph_data = Vec::new();
    let (time_data, visits_data): (Vec<i64>, Vec<f64>) = data
        .iter()
        .enumerate()
        .map(|(_i, analytics)| {
            Ok::<_, eyre::Error>((analytics.time, analytics.visits))
        })
        .fold::<eyre::Result<(Vec<i64>, Vec<f64>)>, _>(
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

#[derive(Deserialize, Default)]
pub struct Options {
    pub uri_filter: Option<String>,
    pub from: Option<Time>,
    pub to: Option<Time>,
}

pub async fn graph_analytics(database: &DatabaseInstance, options: Options) -> eyre::Result<Graph> {
    let analytics_data = get_analytics_data(database, options).await?;
    let data = graph_data(analytics_data)?;
    Ok(Graph { data })
}

#[derive(Serialize)]
pub struct Graph {
    pub data: GraphData,
}

///
/// `timestamp` is the unix timestamp in seconds.
/// ```json
/// [
///     [timestamp, timestamp, ...],
///     [visits, visits, ...]
/// ]
/// ```
pub(super) type GraphData = Vec<Vec<minijinja::value::Value>>;
