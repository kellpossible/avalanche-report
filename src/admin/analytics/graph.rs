use crate::{analytics::get_time_bounds, database::Database, types::Time};
use eyre::{Context, ContextCompat};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, Copy)]
struct AnalyticsData {
    /// Time in seconds since epoch.
    time: i64,
    visits: u32,
}

async fn get_analytics_data(
    database: &Database,
    options: Options,
) -> eyre::Result<Vec<AnalyticsData>> {
    match (options.from, options.to) {
        (Some(from), Some(to)) => {
            if *to < *from {
                eyre::bail!("Invalid options to: {to} should not be less than from: {from}");
            }
        }
        _ => {}
    }
    let (min, max) = if let Some(time_bounds) = get_time_bounds(database).await? {
        time_bounds
    } else {
        return Ok(vec![]);
    };

    let from_min: Time = options
        .from
        .map(|from| min.max(*from).into())
        .unwrap_or(min);
    let to_max: Time = options.to.map(|to| max.min(*to).into()).unwrap_or(max);

    let window = (to_max - from_min) / (options.resolution as f64);
    let window = if window.is_zero() {
        time::Duration::milliseconds(1)
    } else {
        window
    };

    let mut from = from_min;
    let mut to: Time = (*from_min + window).min(*to_max).into();

    let options = std::sync::Arc::new(options);
    let mut data = Vec::with_capacity(options.resolution);
    loop {
        let options_conn = options.clone();

        let mut query = sqlx::QueryBuilder::new("SELECT SUM(visits) FROM analytics WHERE ");
        query.push("analytics.time >= ").push_bind(from);

        if to == to_max {
            query.push(" AND analytics.time <= ");
        } else {
            query.push(" AND analytics.time < ");
        }
        query.push_bind(to);

        if let Some(ref uri_filter) = options_conn.uri_filter {
            query.push(" AND analytics.uri GLOB ").push_bind(uri_filter);
        }

        let visits: u32 = query.build().fetch_one(database).await?.try_get(0)?;

        let time = *from + (window.checked_div(2).wrap_err("Error dividing duration")?);

        data.push(AnalyticsData {
            time: time.unix_timestamp(),
            visits,
        });

        from = to;
        to = (*from + window).into();
        if *to >= *to_max {
            break;
        }
    }

    Ok(data)
}

fn graph_data(data: Vec<AnalyticsData>) -> eyre::Result<GraphData> {
    if data.is_empty() {
        return Ok(vec![vec![], vec![]]);
    }

    let mut graph_data = Vec::new();
    let (time_data, visits_data): (Vec<i64>, Vec<f64>) = data
        .iter()
        .enumerate()
        .map(|(_i, analytics)| Ok::<_, eyre::Error>((analytics.time, analytics.visits)))
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

#[derive(Debug, Deserialize)]
pub struct Options {
    pub uri_filter: Option<String>,
    pub from: Option<Time>,
    pub to: Option<Time>,
    pub resolution: usize,
}

pub async fn graph_analytics(database: &Database, options: Options) -> eyre::Result<Graph> {
    let analytics_data = get_analytics_data(database, options)
        .await
        .wrap_err("Error getting analytics data")?;
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
