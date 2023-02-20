use std::num::NonZeroU32;

use crate::{
    database::{DatabaseInstance, DATETIME_FORMAT},
    serde::string,
    templates::render,
    types::Time,
};
use axum::{extract::State, response::Response, Extension};
use http::Uri;
use sea_query::{Alias, Expr, IntoIden, Order, Query, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::Serialize;

use crate::{
    analytics::AnalyticsIden,
    error::{map_eyre_error, map_std_error},
    state::AppState,
    templates::TemplatesWithContext,
};

#[derive(Serialize)]
struct AnalyticsPage {
    summaries_duration: Vec<SummariesDuration>,
    batch_rate: NonZeroU32,
}

#[derive(Serialize)]
struct SummariesDuration {
    formatted_duration: Option<String>,
    from: Option<Time>,
    to: Time,
    summaries: Vec<Summary>,
}

#[derive(Serialize)]
struct Summary {
    #[serde(with = "string")]
    uri: Uri,
    visits: u64,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(database): Extension<DatabaseInstance>,
    State(state): State<AppState>,
) -> axum::response::Result<Response> {
    let mut summaries_duration = Vec::new();
    for duration in [
        Some(time::Duration::minutes(10)),
        Some(time::Duration::hours(24)),
        Some(time::Duration::days(7)),
        Some(time::Duration::days(30)),
        None,
    ] {
        let to = Time::now_utc();
        let from = duration.as_ref().map(|duration| to - *duration);
        let summaries = get_analytics(&database, from)
            .await
            .map_err(map_eyre_error)?;

        let formatted_duration =
            Option::transpose(duration.map(|duration| std::time::Duration::try_from(duration)))
                .map_err(map_std_error)?
                .map(|duration| humantime::format_duration(duration).to_string());

        summaries_duration.push(SummariesDuration {
            formatted_duration,
            summaries,
            from,
            to,
        })
    }

    let page = AnalyticsPage {
        summaries_duration,
        batch_rate: state.options.analytics_batch_rate,
    };

    render(&templates.environment, "admin/analytics.html", &page)
}

async fn get_analytics(
    database: &DatabaseInstance,
    from: Option<Time>,
) -> eyre::Result<Vec<Summary>> {
    database
        .interact(move |conn| {
            let mut query = Query::select();
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
