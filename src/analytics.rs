use std::{collections::HashMap, sync::mpsc::RecvError, num::{NonZeroU64, NonZeroU32}};

use axum::{extract::State, middleware::Next, response::Response};
use deadpool_sqlite::rusqlite::Row;
use eyre::Context;
use futures::StreamExt;
use governor::{state::StreamRateLimitExt, Quota, RateLimiter};
use http::{Request, Uri, StatusCode};
use sea_query::{Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{database::Database, state::AppState};

#[sea_query::enum_def]
pub struct Analytics {
    pub id: uuid::Uuid,
    pub uri: Uri,
    pub visits: u64,
    pub time: time::OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct Event {
    uri: Uri,
}

async fn process_analytics_events(events: Vec<Event>, database: &Database) -> eyre::Result<()> {
    let db = database.get().await?;

    let mut events_count: HashMap<Uri, u64> = HashMap::with_capacity(1);
    for event in events {
        events_count
            .entry(event.uri)
            .and_modify(|e| *e += 1)
            .or_insert(1);
    }

    db.interact(move |conn| {
        let mut query = Query::insert();

        query.into_table(AnalyticsIden::Table).columns([
            AnalyticsIden::Id,
            AnalyticsIden::Uri,
            AnalyticsIden::Visits,
            AnalyticsIden::Time,
        ]);

        for (uri, count) in events_count {
            query.values_panic([
                uuid::Uuid::new_v4().into(),
                uri.to_string().into(),
                count.into(),
                time::OffsetDateTime::now_utc().into(),
            ]);
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        conn.execute(&sql, &*values.as_params())?;
        Ok::<(), eyre::Error>(())
    })
    .await
    .map_err(|error| eyre::eyre!("{}", error))??;

    Ok(())
}

/// `batch_rate` is the rate that batches can be submitted to the database (per minute).
pub async fn process_analytics(database: Database, mut rx: mpsc::Receiver<Event>, batch_rate: NonZeroU32) {
    let limiter = RateLimiter::direct(Quota::per_minute(batch_rate));

    'main: loop {
        if let Some(first_event) = rx.recv().await {
            limiter.until_ready().await;

            let mut buffer: Vec<Event> = Vec::with_capacity(50);
            buffer.push(first_event);
            'buffer: loop {
                match rx.try_recv() {
                    Ok(event) => buffer.push(event),
                    Err(error) => match error {
                        mpsc::error::TryRecvError::Empty => break 'buffer,
                        mpsc::error::TryRecvError::Disconnected => break 'main,
                    },
                }
            }

            process_analytics_events(buffer, &database)
                .await
                .wrap_err("Error processing analytics events")
                .unwrap_or_else(|error| tracing::error!("{error}"));
        }
    }
}

pub fn channel() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
    mpsc::channel(1000)
}

/// Middleware for performing analytics on incoming requests.
pub async fn middleware<B>(state: State<AppState>, request: Request<B>, next: Next<B>) -> Response {
    let uri = request.uri().clone();
    let response = next.run(request).await;
    let uri = match response.status() {
        StatusCode::NOT_FOUND => "/404".parse().expect("unable to parse uri"),
        _ => uri,
    };
    let event = Event {
        uri,
    };
    state
        .analytics_sx
        .try_send(event)
        .unwrap_or_else(|error| tracing::debug!("Error sending to analytics processor: {error}"));
    response
}
