use std::num::NonZeroU32;

use crate::{serde::string, templates::render};
use axum::{extract::State, response::Response, Extension};
use http::Uri;
use sea_query::{Alias, Expr, IntoIden, Order, Query, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::Serialize;

use crate::{
    analytics::AnalyticsIden,
    database::Database,
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
    summaries: Vec<Summary>,
}

#[derive(Serialize)]
struct Summary {
    #[serde(with = "string")]
    uri: Uri,
    visits: u64,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<Response> {
    let mut summaries_duration = Vec::new();
    for duration in [
        Some(time::Duration::minutes(10)),
        Some(time::Duration::hours(24)),
        Some(time::Duration::days(7)),
        Some(time::Duration::days(30)),
        None,
    ] {
        let from = duration
            .as_ref()
            .map(|duration| time::OffsetDateTime::now_utc() - *duration);
        let summaries = get_analytics(&state.database, from)
            .await
            .map_err(map_eyre_error)?;

        let formatted_duration =
            Option::transpose(duration.map(|duration| std::time::Duration::try_from(duration)))
                .map_err(map_std_error)?
                .map(|duration| humantime::format_duration(duration).to_string());

        summaries_duration.push(SummariesDuration {
            formatted_duration,
            summaries,
        })
    }

    let page = AnalyticsPage {
        summaries_duration,
        batch_rate: state.options.analytics_batch_rate,
    };

    let template = templates
        .environment
        .get_template("admin/analytics.html")
        .map_err(map_std_error)?;

    render(&template, &page)
}

async fn get_analytics(
    database: &Database,
    from: Option<time::OffsetDateTime>,
) -> eyre::Result<Vec<Summary>> {
    let db = database.get().await?;
    db.interact(move |conn| {
        let mut query = Query::select();
        let visitor_sum = Alias::new("vs");

        query
            .distinct()
            .columns([AnalyticsIden::Uri])
            .expr(Expr::cust(&format!(
                "SUM(\"{}\") as vs",
                AnalyticsIden::Visits.into_iden().to_string()
            )))
            .group_by_col(AnalyticsIden::Uri)
            .order_by(visitor_sum, Order::Desc)
            .limit(20)
            .from(AnalyticsIden::Table);

        if let Some(from) = from {
            query.and_where(Expr::col(AnalyticsIden::Time).gt(SimpleExpr::Value(from.into())));
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
    .await
    .map_err(|error| eyre::eyre!("{error}"))?
}
