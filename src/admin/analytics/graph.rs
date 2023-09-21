use crate::{
    analytics::{get_time_bounds, Analytics, AnalyticsIden},
    database::{Database, DatabaseInstance},
    types::Time,
};
use eyre::{Context, ContextCompat};
use sea_query::{Expr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
struct AnalyticsData {
    /// Time in seconds since epoch.
    time: i64,
    visits: u32,
}

async fn get_analytics_data(
    database: &Database,
    options: Options,
) -> eyre::Result<Vec<AnalyticsData>> {
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

    let mut from = from_min;
    let mut to: Time = (*from_min + window).into();

    let options = std::sync::Arc::new(options);
    let mut data = Vec::with_capacity(options.resolution);
    loop {
        let options_conn = options.clone();
        let visits = database
            .get()
            .await?
            .interact::<_, eyre::Result<_>>(move |conn| {
                let mut query = sea_query::Query::select();
                let from_where = Expr::col(AnalyticsIden::Time).gte(from);
                let to_where = if to == to_max {
                    Expr::col(AnalyticsIden::Time).lte(to)
                } else {
                    Expr::col(AnalyticsIden::Time).lt(to)
                };
                query
                    .expr(Expr::col(AnalyticsIden::Visits).sum())
                    .from(Analytics::TABLE)
                    .and_where(from_where)
                    .and_where(to_where);

                if let Some(ref uri_filter) = options_conn.uri_filter {
                    query.and_where(Expr::cust_with_values(
                        &format!("{} GLOB ?", AnalyticsIden::Uri.as_ref()),
                        [uri_filter],
                    ));
                }
                let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
                let mut statement = conn.prepare_cached(&sql)?;
                let visits: u32 = statement
                    .query_row(&*values.as_params(), |row| {
                        let visits: Option<i64> = row.get(0)?;
                        Ok(visits.unwrap_or_default())
                    })
                    .wrap_err_with(|| {
                        format!("Error executing query {sql} with values {values:?}")
                    })?
                    .try_into()
                    .wrap_err("Error converting visits from i32 into u32")?;

                Ok(visits)
            })
            .await??;
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

#[derive(Deserialize)]
pub struct Options {
    pub uri_filter: Option<String>,
    pub from: Option<Time>,
    pub to: Option<Time>,
    pub resolution: usize,
}

pub async fn graph_analytics(database: &Database, options: Options) -> eyre::Result<Graph> {
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
