use std::num::NonZeroU32;

use crate::{database::Database, templates::render, types::Time};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use eyre::Context;
use futures::TryStreamExt;
use http::{header::CONTENT_TYPE, Uri};
use serde::{Deserialize, Serialize};
use sqlx::{Execute, Row};
use time::OffsetDateTime;
use utils::serde::rfc3339_option;

use crate::{error::map_eyre_error, state::AppState, templates::TemplatesWithContext};

mod serde_duration_secons {}

mod duration_option {
    use serde::{de::Visitor, Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Duration {
        Duration(time::Duration),
        AllTime,
        Custom,
    }

    impl Duration {
        pub fn duration(&self) -> Option<time::Duration> {
            match self {
                Duration::Duration(duration) => Some(*duration),
                Duration::AllTime => None,
                Duration::Custom => None,
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
                "custom" => Ok(Duration::Custom),
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
                Duration::Custom => serializer.serialize_str("custom"),
            }
        }
    }
}

pub(super) use duration_option::Duration;

use super::graph::{self, graph_analytics, Graph};

#[derive(Clone, Serialize, PartialEq)]
pub(super) struct DurationOption {
    pub duration: Duration,
    pub name: String,
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
    #[serde(with = "http_serde::uri")]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(default)]
pub struct Query {
    duration: Option<Duration>,
    #[serde(with = "rfc3339_option")]
    from: Option<time::OffsetDateTime>,
    #[serde(with = "rfc3339_option")]
    to: Option<time::OffsetDateTime>,
    /// A filter with glob support, like `/forecast/*`
    uri_filter: Option<String>,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    axum::extract::Query(mut query): axum::extract::Query<Query>,
    headers: headers::HeaderMap,
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
        ("10 minutes", time::Duration::minutes(10).into()),
        ("24 hours", time::Duration::hours(24).into()),
        ("7 days", time::Duration::days(7).into()),
        ("1 month", time::Duration::days(30).into()),
        ("1 year", time::Duration::days(365).into()),
        ("All Time", Duration::AllTime),
        ("Custom", Duration::Custom),
    ]
    .into_iter()
    .map(|(name, duration)| DurationOption {
        duration,
        name: name.to_owned(),
    })
    .collect();

    let custom_duration_option = duration_options
        .last()
        .expect("Expected at least one duration option")
        .clone();

    let (from, to, duration_option) = match (query.from, query.to, query.duration) {
        (Some(from), Some(to), None) | (Some(from), Some(to), Some(Duration::Custom)) => {
            (Some(from), Some(to), custom_duration_option)
        }
        (Some(from), None, None) | (Some(from), None, Some(Duration::Custom)) => {
            (Some(from), None, custom_duration_option)
        }
        (None, Some(to), None) | (None, Some(to), Some(Duration::Custom)) => {
            (None, Some(to), custom_duration_option)
        }
        (None, Some(_), Some(Duration::AllTime))
        | (Some(_), None, Some(Duration::AllTime))
        | (Some(_), Some(_), Some(Duration::AllTime)) => {
            return Err(map_eyre_error(eyre::eyre!(
                "Cannot specify `from` or `to`, and a `duration` of `all-time`"
            ))
            .into());
        }
        (Some(_), Some(_), Some(Duration::Duration(_))) => {
            return Err(map_eyre_error(eyre::eyre!(
                "Cannot specify `from` and `to`, and a `duration`"
            ))
            .into());
        }
        (None, None, Some(Duration::Custom)) => {
            return Err(map_eyre_error(eyre::eyre!(
                "Cannot specify `duration` as `custom` without also specifying either `from` or `to`"
            )).into());
        }
        (Some(from), None, Some(Duration::Duration(duration))) => {
            let option = duration_options
                .iter()
                .find(|option| option.duration == Duration::Duration(duration))
                .cloned()
                .unwrap_or(custom_duration_option);
            (Some(from), Some(from + duration), option)
        }
        (None, Some(to), Some(Duration::Duration(duration))) => {
            let option = duration_options
                .iter()
                .find(|option| option.duration == Duration::Duration(duration))
                .cloned()
                .unwrap_or(custom_duration_option);
            (Some(to - duration), Some(to), option)
        }
        (None, None, Some(Duration::AllTime)) => {
            let option = duration_options
                .iter()
                .find(|option| option.duration == Duration::AllTime)
                .cloned()
                .expect("Expected all-time to be in duration options");
            (None, None, option)
        }
        (None, None, Some(Duration::Duration(duration))) => {
            let option = duration_options
                .iter()
                .find(|option| option.duration == Duration::Duration(duration))
                .cloned()
                .unwrap_or(custom_duration_option);
            (Some(OffsetDateTime::now_utc() - duration), None, option)
        }
        (None, None, None) => {
            let option = duration_options
                .get(1)
                .expect("Expected duration option to be present")
                .clone();
            let from = OffsetDateTime::now_utc()
                - option
                    .duration
                    .duration()
                    .expect("Expected default option to have a duration");
            (Some(from), None, option)
        }
    };

    // Don't let the user select Custom
    let duration_options = if duration_option.duration != Duration::Custom {
        duration_options
            .into_iter()
            .filter(|option| option.duration != Duration::Custom)
            .collect()
    } else {
        duration_options
    };

    match (query.from, query.to) {
        (Some(from), Some(to)) => {
            if to < from {
                return Err(map_eyre_error(eyre::eyre!(
                    "Invalid query parameters to: {to} should not be less than from: {from}"
                ))
                .into());
            }
        }
        _ => {}
    }

    let from = from.map(Time::from);
    let to = to.map(Time::from);

    let summaries = get_analytics(
        &state.database,
        from.map(Into::into),
        to.map(Into::into),
        query.uri_filter.clone(),
    )
    .await
    .map_err(map_eyre_error)?;

    let summaries_duration = SummariesDuration {
        duration_option,
        to: to.unwrap_or(Time::now_utc()),
        from,
        summaries,
    };

    let graph = graph_analytics(
        &state.database,
        graph::Options {
            to,
            from,
            uri_filter: query.uri_filter.clone(),
            resolution: 512,
        },
    )
    .await
    .map_err(map_eyre_error)?;

    let page = AnalyticsPage {
        duration_options,
        summaries_duration,
        batch_rate: state.options.analytics.event_batch_rate,
        graph,
        query: query.clone(),
    };

    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        let content_type = content_type
            .to_str()
            .wrap_err("Invalid content-type header")
            .map_err(map_eyre_error)?;

        if content_type == "application/json" {
            return Ok(Json(page).into_response());
        }
    }

    let template = headers
        .get("X-Template")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("admin/analytics.html");

    Ok(render(&templates.environment, template, &page).map_err(map_eyre_error)?)
}

async fn get_analytics(
    database: &Database,
    from: Option<Time>,
    to: Option<Time>,
    uri_filter: Option<String>,
) -> eyre::Result<Vec<Summary>> {
    let mut query = sqlx::QueryBuilder::new(sqlx::query!("SELECT DISTINCT uri, SUM(visits) as visitor_sum FROM analytics WHERE uri NOT LIKE '/admin/analytics%' ").sql());
    if let Some(from) = from {
        query.push("AND analytics.time >= ");
        query.push_bind(from);
    }
    if let Some(to) = to {
        query.push("AND analytics.time <= ");
        query.push_bind(to);
    }

    if let Some(uri_filter) = uri_filter {
        query.push("AND uri GLOB ");
        query.push_bind(uri_filter);
    }
    query.push("GROUP BY uri ORDER BY visitor_sum DESC LIMIT 20");
    query
        .build()
        .fetch(database)
        .map_err(eyre::Error::from)
        .and_then(|row| async move {
            let uri = row.try_get::<String, _>("uri")?.parse()?;
            let visits = row.try_get("visitor_sum")?;
            Ok(Summary { uri, visits })
        })
        .try_collect()
        .await
}
