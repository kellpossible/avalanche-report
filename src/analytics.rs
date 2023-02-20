use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

use axum::{extract::State, middleware::Next, response::Response};
use eyre::Context;
use futures::{lock::Mutex, StreamExt};
use governor::{state::StreamRateLimitExt, Quota, RateLimiter};
use http::{Request, StatusCode};
use nonzero_ext::nonzero;
use rusqlite::Row;
use sea_query::{Query, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::{
    database::{Database, DATETIME_FORMAT},
    state::AppState,
    types::{self, Uri},
};

#[sea_query::enum_def]
pub struct Analytics {
    pub id: uuid::Uuid,
    pub uri: Uri,
    pub visits: u64,
    pub time: types::Time,
}

impl TryFrom<&Row<'_>> for Analytics {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        let id: Uuid = row.get(AnalyticsIden::Id.as_ref())?;
        let uri = row.get(AnalyticsIden::Uri.as_ref())?;
        let visits = row.get(AnalyticsIden::Visits.as_ref())?;
        let time = row.get(AnalyticsIden::Time.as_ref())?;

        Ok(Analytics {
            id,
            uri,
            visits,
            time,
        })
    }
}

impl AsRef<str> for AnalyticsIden {
    fn as_ref(&self) -> &str {
        match self {
            AnalyticsIden::Table => "analytics",
            AnalyticsIden::Id => "id",
            AnalyticsIden::Uri => "uri",
            AnalyticsIden::Visits => "visits",
            AnalyticsIden::Time => "time",
        }
    }
}

impl Analytics {
    pub const COLUMNS: [AnalyticsIden; 4] = [
        AnalyticsIden::Id,
        AnalyticsIden::Uri,
        AnalyticsIden::Visits,
        AnalyticsIden::Time,
    ];
    pub const TABLE: AnalyticsIden = AnalyticsIden::Table;

    pub fn values(&self) -> [SimpleExpr; 4] {
        [
            self.id.into(),
            self.uri.to_string().into(),
            self.visits.into(),
            self.time
                .format(&DATETIME_FORMAT)
                .expect("Error formatting time")
                .into(),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    uri: Uri,
}

async fn process_analytics_events(
    accumulator: EventsAccumulator,
    database: &Database,
) -> eyre::Result<()> {
    if accumulator.is_empty() {
        return Ok(());
    }
    let db = database.get().await?;

    db.interact(move |conn| {
        let mut query = Query::insert();

        query
            .into_table(AnalyticsIden::Table)
            .columns(Analytics::COLUMNS);

        for (uri, visits) in accumulator {
            let analytics = Analytics {
                id: uuid::Uuid::new_v4(),
                uri,
                visits,
                time: time::OffsetDateTime::now_utc().into(),
            };
            query.values(analytics.values())?;
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        conn.execute(&sql, &*values.as_params())?;
        Ok::<(), eyre::Error>(())
    })
    .await
    .map_err(|error| eyre::eyre!("{}", error))??;

    Ok(())
}

type EventsAccumulator = HashMap<Uri, u64>;

#[tracing::instrument(skip_all)]
async fn process_accumulated_events(
    database: &Database,
    rx: mpsc::Receiver<EventsAccumulator>,
    batch_rate: NonZeroU32,
) {
    let limiter = RateLimiter::direct(Quota::per_hour(batch_rate).allow_burst(nonzero!(1u32)));

    ReceiverStream::from(rx)
        .ratelimit_stream(&limiter)
        .for_each(|accumulator| async {
            process_analytics_events(accumulator, database)
                .await
                .wrap_err("Error processing analytics events")
                .unwrap_or_else(|error| tracing::error!("{error}"));
        })
        .await;
}

async fn notify_received_events_for_processing(
    batch_tx: mpsc::Sender<EventsAccumulator>,
    events_accumulator: Arc<Mutex<EventsAccumulator>>,
    mut batch_events_received_rx: watch::Receiver<()>,
) {
    loop {
        match batch_tx.reserve().await {
            Ok(permit) => {
                batch_events_received_rx
                    .changed()
                    .await
                    .expect("failed to check whether events have been received");
                let mut events_accumulator = events_accumulator.lock().await;
                if !events_accumulator.is_empty() {
                    permit.send(events_accumulator.clone());
                    events_accumulator.clear();
                } else {
                }
            }
            Err(error) => {
                tracing::warn!("batch_tx reserve error: {}", error);
                return;
            }
        }
    }
}

/// `batch_rate` is the rate that batches can be submitted to the database (per hour).
#[tracing::instrument(skip_all)]
pub async fn process_analytics(
    database: Database,
    mut rx: mpsc::Receiver<Event>,
    batch_rate: NonZeroU32,
) {
    // Care must be taken not to hold a lock on this over an await.
    let events_accumulator: Arc<Mutex<EventsAccumulator>> =
        Arc::new(Mutex::new(EventsAccumulator::with_capacity(1)));
    let (batch_tx, batch_rx) = mpsc::channel::<EventsAccumulator>(1);

    tokio::task::spawn(async move {
        process_accumulated_events(&database, batch_rx, batch_rate).await;
    });

    let (events_received_tx, events_received_rx) = watch::channel(());
    let batch_events_accumulator = events_accumulator.clone();
    let batch_events_received_rx = events_received_rx.clone();
    tokio::task::spawn(async move {
        notify_received_events_for_processing(
            batch_tx,
            batch_events_accumulator,
            batch_events_received_rx,
        )
        .await;
    });
    loop {
        if let Some(event) = rx.recv().await {
            let mut events_accumulator = events_accumulator.lock().await;
            events_accumulator
                .entry(event.uri)
                .and_modify(|e| *e += 1)
                .or_insert(1);
            events_received_tx
                .send(())
                .expect("failed to notify events received");
        } else {
            tracing::warn!("events_accumulator channel closed, stopping analytics processor");
            return;
        }
    }
}

pub fn channel() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
    mpsc::channel(100)
}

/// Middleware for performing analytics on incoming requests.
#[tracing::instrument(skip_all)]
pub async fn middleware<B>(state: State<AppState>, request: Request<B>, next: Next<B>) -> Response {
    let uri = Uri::from(request.uri().clone());
    let response = next.run(request).await;
    let uri = match response.status() {
        StatusCode::NOT_FOUND => "/404".parse().expect("unable to parse uri"),
        _ => uri,
    };
    let event = Event { uri };
    state
        .analytics_sx
        .try_send(event)
        .unwrap_or_else(|error| tracing::warn!("Error sending to analytics processor: {error}"));
    response
}
