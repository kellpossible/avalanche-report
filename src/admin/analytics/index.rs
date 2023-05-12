use std::num::NonZeroU32;

use crate::{
    database::{DatabaseInstance, DATETIME_FORMAT},
    serde::string,
    templates::render,
    types::Time,
};
use axum::{extract::State, response::Response, Extension};
use http::Uri;
use sea_query::{Alias, Expr, IntoIden, Order, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::{Deserialize, Serialize};

use crate::{
    analytics::AnalyticsIden, error::map_eyre_error, state::AppState,
    templates::TemplatesWithContext,
};

mod duration_option {
    use serde::{de::Visitor, Deserialize, Serialize};

    #[derive(Clone, Copy)]
    pub enum Duration {
        Duration(time::Duration),
        AllTime,
    }

    impl Duration {
        pub fn duration(&self) -> Option<time::Duration> {
            match self {
                Duration::Duration(duration) => Some(*duration),
                Duration::AllTime => None,
            }
        }
    }

    impl From<time::Duration> for Duration {
        fn from(value: time::Duration) -> Self {
            Self::Duration(value)
        }
    }

    struct MyVisitor;
    impl<'de> Visitor<'de> for MyVisitor {
        type Value = Duration;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                f,
                "Expecting either duration in seconds: 213451 or \"all-time\""
            )
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let duration = time::Duration::seconds(v.try_into().map_err(|error| {
                serde::de::Error::custom(format!(
                    "Unable to parse duration as seconds i64: {error}"
                ))
            })?);
            Ok(Duration::Duration(duration))
        }

        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match v {
                "all-time" => Ok(Duration::AllTime),
                _ => self.visit_u64(v.parse().map_err(|error| {
                    serde::de::Error::custom(format!(
                        "Unable to parse duration as seconds: {error}"
                    ))
                })?),
            }
        }
    }
    impl<'de> Deserialize<'de> for Duration {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let visitor = MyVisitor;
            deserializer.deserialize_any(visitor)
        }
    }

    impl Serialize for Duration {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            match self {
                Duration::Duration(duration) => {
                    let whole_seconds = duration.whole_seconds();
                    if whole_seconds < 0 {
                        return Err(serde::ser::Error::custom("Duration cannot be negative"));
                    }
                    let duration_seconds: u64 = whole_seconds as u64;
                    serializer.serialize_u64(duration_seconds)
                }
                Duration::AllTime => serializer.serialize_str("all-time"),
            }
        }
    }
}

pub(super) use duration_option::Duration;

use super::graph::{self, graph_analytics, Graph};

#[derive(Clone, Serialize)]
pub(super) struct DurationOption {
    duration: Duration,
    name: String,
}

impl DurationOption {
    fn from_kind(kind: Duration) -> Self {
        let name: String = match &kind {
            Duration::Duration(duration) => {
                let duration_string = if let Ok(duration) = (*duration).try_into() {
                    humantime::format_duration(duration).to_string()
                } else {
                    format!("{} seconds", duration.whole_seconds())
                };
                format!("Past {duration_string}")
            }
            Duration::AllTime => "All Time".to_string(),
        };

        Self {
            name,
            duration: kind,
        }
    }
}

#[derive(Serialize)]
struct SummariesDuration {
    duration_option: DurationOption,
    to: Time,
    from: Option<Time>,
    summaries: Vec<Summary>,
}

#[derive(Serialize)]
struct Summary {
    #[serde(with = "string")]
    uri: Uri,
    visits: u64,
}

#[derive(Serialize)]
struct AnalyticsPage {
    duration_options: Vec<DurationOption>,
    summaries_duration: SummariesDuration,
    batch_rate: NonZeroU32,
    graph: Graph,
    query: Query,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Query {
    duration: Option<Duration>,
    /// A filter with glob support, like `/forecast/*`
    uri_filter: Option<String>,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(database): Extension<DatabaseInstance>,
    axum::extract::Query(mut query): axum::extract::Query<Query>,
    headers: axum::headers::HeaderMap,
    State(state): State<AppState>,
) -> axum::response::Result<Response> {
    let empty_uri_filter: bool = query
        .uri_filter
        .as_ref()
        .map(|s| s.is_empty())
        .unwrap_or(false);
    if empty_uri_filter {
        query.uri_filter = None;
    }
    let duration_options: Vec<DurationOption> = [
        Duration::from(time::Duration::minutes(10)),
        Duration::from(time::Duration::hours(24)),
        Duration::from(time::Duration::days(7)),
        Duration::from(time::Duration::days(30)),
        Duration::AllTime,
    ]
    .into_iter()
    .map(DurationOption::from_kind)
    .collect();

    let selected_duration = query
        .duration
        .map(DurationOption::from_kind)
        .unwrap_or_else(|| {
            duration_options
                .get(1)
                .expect("Expected duration option to be present")
                .clone()
        });
    let duration = selected_duration.duration.duration();
    let to = Time::now_utc();
    let from = duration.as_ref().map(|duration| to - *duration);

    let summaries = get_analytics(&database, from, query.uri_filter.clone())
        .await
        .map_err(map_eyre_error)?;

    let summaries_duration = SummariesDuration {
        duration_option: selected_duration,
        to,
        from,
        summaries,
    };

    let graph = graph_analytics(
        &database,
        graph::Options {
            to: Some(to),
            from,
            uri_filter: query.uri_filter.clone(),
            ..graph::Options::default()
        },
    )
    .await
    .map_err(map_eyre_error)?;

    let page = AnalyticsPage {
        duration_options,
        summaries_duration,
        batch_rate: state.options.analytics_batch_rate,
        graph,
        query: query.clone(),
    };

    let template = headers
        .get("X-Template")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("admin/analytics.html");

    render(&templates.environment, template, &page).map_err(map_eyre_error)
}

async fn get_analytics(
    database: &DatabaseInstance,
    from: Option<Time>,
    uri_filter: Option<String>,
) -> eyre::Result<Vec<Summary>> {
    database
        .interact(move |conn| {
            let mut query = sea_query::Query::select();
            let visitor_sum = Alias::new("vs");

            query
                .distinct()
                .columns([AnalyticsIden::Uri])
                .expr(Expr::cust(&format!(
                    "SUM(\"{}\") as vs",
                    AnalyticsIden::Visits.into_iden().to_string()
                )))
                .and_where(Expr::col(AnalyticsIden::Uri).not_like("/admin/analytics%"))
                .group_by_col(AnalyticsIden::Uri)
                .order_by(visitor_sum, Order::Desc)
                .limit(20)
                .from(AnalyticsIden::Table);

            if let Some(from) = from {
                let from_time_string = from.format(&DATETIME_FORMAT)?;
                query.and_where(
                    Expr::col(AnalyticsIden::Time).gt(SimpleExpr::Value(from_time_string.into())),
                );
            }

            if let Some(uri_filter) = uri_filter {
                query.and_where(Expr::cust_with_values(
                    &format!("{} GLOB ?", AnalyticsIden::Uri.as_ref()),
                    [uri_filter],
                ));
            }

            let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);

            let mut statement = conn.prepare_cached(&sql)?;
            let mut rows = statement.query(&*values.as_params())?;
            let mut summaries: Vec<Summary> = Vec::new();
            while let Some(row) = rows.next()? {
                let uri: String = row.get_unwrap(0);
                let visits = row.get_unwrap(1);
                let summary = Summary {
                    uri: uri.parse()?,
                    visits,
                };
                summaries.push(summary);
            }

            Ok::<_, eyre::Error>(summaries)
        })
        .await?
}
