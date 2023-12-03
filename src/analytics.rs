use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

use average::WeightedMean;
use axum::{extract::State, middleware::Next, response::Response};
use cronchik::CronSchedule;
use eyre::{Context, ContextCompat};
use futures::{lock::Mutex, StreamExt};
use governor::{state::StreamRateLimitExt, Quota, RateLimiter};
use http::{Request, StatusCode};
use nonzero_ext::nonzero;
use rusqlite::Row;
use sea_query::{ConditionalStatement, Expr, Func, Order, Query, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    database::{Database, DATETIME_FORMAT},
    isbot::IsBot,
    state::AppState,
    types::{self, Uri},
};

#[derive(Serialize, Clone, Debug)]
#[sea_query::enum_def]
pub struct Analytics {
    pub id: uuid::Uuid,
    pub uri: String,
    pub visits: u32,
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

#[derive(Debug, Serialize)]
struct CompactOperation {
    delete: Vec<Analytics>,
    new: Analytics,
}

fn compact_operations(map: HashMap<String, Vec<Analytics>>) -> eyre::Result<Vec<CompactOperation>> {
    map.into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(key, entries)| {
            let first = entries.first().expect("Expected at least one entry");
            let start_timestamp = first.time.unix_timestamp();
            let mean: WeightedMean = entries
                .iter()
                .map(|entry| {
                    // Work in the space of the duration of the window to improve precision.
                    let time_seconds = (entry.time.unix_timestamp() - start_timestamp) as f64;
                    (time_seconds, entry.visits as f64)
                })
                .collect();

            let visits = mean.sum_weights().round() as u32;
            // Transform back to unix timestamp.
            let timestamp = (mean.mean().round() as i64) + start_timestamp;
            let time = OffsetDateTime::from_unix_timestamp(timestamp)?;
            let new_entry = Analytics {
                id: Uuid::new_v4(),
                uri: key,
                visits,
                time: time.into(),
            };

            Ok(CompactOperation {
                delete: entries,
                new: new_entry,
            })
        })
        .collect::<eyre::Result<_>>()
}

pub async fn get_time_bounds(
    database: &Database,
) -> eyre::Result<Option<(types::Time, types::Time)>> {
    let (sql, values) = Query::select()
        .expr(Expr::col(AnalyticsIden::Time).min())
        .expr(Expr::col(AnalyticsIden::Time).max())
        .from(AnalyticsIden::Table)
        .build_rusqlite(SqliteQueryBuilder);
    let first = database
        .get()
        .await?
        .interact::<_, eyre::Result<_>>(move |conn| {
            let mut statement = conn.prepare_cached(&sql)?;
            let item = Option::transpose(
                statement
                    .query_map(&*values.as_params(), |row| {
                        let min = row.get(0)?;
                        let max = row.get(1)?;
                        Ok((min, max))
                    })?
                    .next(),
            )?;
            Ok(item)
        })
        .await??;

    Ok(first)
}

pub struct CompactionConfig {
    pub schedule: CronSchedule,
    pub database: Database,
}

pub fn spawn_compaction_task(CompactionConfig { schedule, database }: CompactionConfig) {
    let span = tracing::error_span!("analytics_compaction");
    tokio::spawn(
        async move {
            loop {
                let next_time = schedule.next_time_from_now();
                let now = OffsetDateTime::now_utc();
                let duration: std::time::Duration = (next_time - now)
                    .try_into()
                    .expect("Unable to convert duration");
                let human_duration = humantime::format_duration(duration.clone());
                tracing::info!("Next analytics compaction in {human_duration}");
                tokio::time::sleep(duration).await;

                if let Err(error) =
                    compact(&database, time::Duration::days(1), time::Duration::days(7))
                        .await
                        .wrap_err("Error performing compaction")
                {
                    tracing::error!("{error}");
                }
            }
        }
        .instrument(span),
    );
}

/// Compact analytics entries in the database.
///
/// Older entries with the same key will be combined within some window. This results in a loss of resolution in the
/// time domain, in exchange for a much smaller database.
pub async fn compact(database: &Database, window: Duration, keep: Duration) -> eyre::Result<()> {
    tracing::info!("Compacting analytics...");

    let min = if let Some((min, _max)) = get_time_bounds(database).await? {
        min
    } else {
        // No analytics
        return Ok(());
    };

    let (sql, values) = Query::select()
        .from(AnalyticsIden::Table)
        .columns(Analytics::COLUMNS)
        .order_by(AnalyticsIden::Time, Order::Desc)
        .limit(1)
        .build_rusqlite(SqliteQueryBuilder);
    let last = database
        .get()
        .await?
        .interact::<_, eyre::Result<_>>(move |conn| {
            let mut statement = conn.prepare_cached(&sql)?;
            let item = Option::transpose(
                statement
                    .query_map(&*values.as_params(), |row| Analytics::try_from(row))?
                    .next(),
            )?;
            Ok(item)
        })
        .await??
        .wrap_err("Expected there to be a last item")?;

    let end_time = *last.time - keep;
    let mut from_time = min;

    loop {
        let mut to_time = *from_time + window;
        if to_time > end_time {
            to_time = end_time;
        }
        let to_time = types::Time::from(to_time);
        tracing::debug!(
            "{} .. {}",
            from_time.format(&Rfc3339)?,
            to_time.format(&Rfc3339)?
        );

        let (sql, values) = Query::select()
            .from(AnalyticsIden::Table)
            .columns(Analytics::COLUMNS)
            .and_where(Expr::col(AnalyticsIden::Time).gte(from_time))
            .and_where(Expr::col(AnalyticsIden::Time).lt(to_time))
            .order_by(AnalyticsIden::Time, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);

        let map = database
            .get()
            .await?
            .interact::<_, eyre::Result<_>>(move |conn| {
                let mut statement = conn.prepare_cached(&sql)?;
                let mut map: HashMap<String, Vec<Analytics>> = HashMap::new();

                for result in statement
                    .query_map(&*values.as_params(), |row| Analytics::try_from(row))
                    .wrap_err("Error performing query")?
                {
                    let item = result?;
                    let entries = map.entry(item.uri.clone()).or_insert_with(|| Vec::new());
                    entries.push(item);
                }

                Ok(map)
            })
            .await??;

        let operations = compact_operations(map)?;
        tracing::debug!("{} operations", operations.len());
        for CompactOperation { delete, new } in &operations {
            tracing::debug!("Replacing {} entries with {new:?}", delete.len());
        }

        for CompactOperation { delete, new } in operations {
            let delete_ids: Vec<Uuid> = delete.into_iter().map(|entry| entry.id).collect();

            database
                .get()
                .await?
                .interact::<_, eyre::Result<_>>(move |conn| {
                    if !delete_ids.is_empty() {
                        let (sql, values) = Query::delete()
                            .from_table(AnalyticsIden::Table)
                            .and_where(Expr::col(AnalyticsIden::Id).is_in(delete_ids))
                            .build_rusqlite(SqliteQueryBuilder);
                        let mut statement = conn.prepare_cached(&sql)?;
                        statement.execute(&*values.as_params())?;
                    }
                    let (sql, values) = Query::insert()
                        .into_table(AnalyticsIden::Table)
                        .columns(Analytics::COLUMNS)
                        .values(new.values())?
                        .build_rusqlite(SqliteQueryBuilder);
                    let mut statement = conn.prepare_cached(&sql)?;
                    statement.execute(&*values.as_params())?;
                    Ok(())
                })
                .await??;
        }
        if *to_time >= end_time {
            break;
        }
        from_time = to_time;
    }

    tracing::info!("Finished compacting analytics!");

    Ok(())
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

type EventsAccumulator = HashMap<String, u32>;

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

/// Receive a notification that a batch of events in `events_accumulator` are ready for processing
/// and submission to the database.
///
/// NOTE: in the future we should be able to improve the performance reducing memory allocations
/// and clones by having a re-usable buffer of capacity limited, pre-allocated EventsAccumulator.
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

/// Receive analytics events from [`middleware()`] via [`channel()`] and accumulate them
/// to be submitted to the database in a rate-limited fashion in order to reduce write load during high
/// traffic situations.
///
/// `batch_rate` is the rate that batches can be submitted to the database (per hour).
#[tracing::instrument(skip_all)]
pub async fn process_analytics(
    database: Database,
    mut rx: mpsc::Receiver<Event>,
    batch_rate: NonZeroU32,
) {
    fn accumulate_event(events_accumulator: &mut EventsAccumulator, event: Event) {
        events_accumulator
            // We intentionally only obtain the path section of the uri,
            // in order to avoid combinatorial explosion of uri parameters
            // in the database.
            .entry(event.uri.path().to_owned())
            .and_modify(|e| *e += 1)
            .or_insert(1);
    }

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
            let mut events_accumulator_guard = events_accumulator.lock().await;
            accumulate_event(&mut events_accumulator_guard, event);
            // Accumulate all events that may be present in the channel while we still hold the
            // events accumulator lock, we are the only consumer of events.
            while let Ok(event) = rx.try_recv() {
                accumulate_event(&mut events_accumulator_guard, event);
            }
            drop(events_accumulator_guard);
            events_received_tx
                .send(())
                .expect("failed to notify events received");
        } else {
            tracing::warn!("events_accumulator channel closed, stopping analytics processor");
            return;
        }
    }
}

/// Channel to use for transmiting analytics information from [`middleware()`] to
/// [`process_analytics()`].
pub fn channel() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
    mpsc::channel(100)
}

/// Middleware for performing analytics on incoming requests.
#[tracing::instrument(skip_all)]
pub async fn middleware<B>(state: State<AppState>, request: Request<B>, next: Next<B>) -> Response {
    let uri = Uri::from(request.uri().clone());
    let is_bot = request
        .extensions()
        .get::<IsBot>()
        .expect("Expected extension IsBot to be available")
        .is_bot();
    let response = next.run(request).await;
    // Skip recording analytics if the request comes from a bot.
    if is_bot {
        return response;
    }
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use insta::assert_json_snapshot;
    use proptest::{
        strategy::{Just, Strategy},
        test_runner::TestRunner,
    };
    use time::OffsetDateTime;
    use uuid::Uuid;

    use crate::types;

    use super::{compact_operations, Analytics};

    #[test]
    fn test_compact_operations_empty() {
        let map: HashMap<String, Vec<Analytics>> = [(
            "/test1",
            vec![Analytics {
                id: Uuid::new_v4(),
                uri: "/test1".to_owned(),
                visits: 1,
                time: "2023-08-09T12:00:00Z".parse().unwrap(),
            }],
        )]
        .into_iter()
        .map(|(key, values)| (key.to_owned(), values))
        .collect();
        let operations = compact_operations(map).unwrap();
        assert_json_snapshot!(operations, @"[]");
    }

    #[test]
    fn test_compact_operations_differnt_uri_empty() {
        let map: HashMap<String, Vec<Analytics>> = [
            (
                "/test1",
                vec![Analytics {
                    id: Uuid::new_v4(),
                    uri: "/test1".to_owned(),
                    visits: 1,
                    time: "2023-08-09T12:00:00Z".parse().unwrap(),
                }],
            ),
            (
                "/test2",
                vec![Analytics {
                    id: Uuid::new_v4(),
                    uri: "/test2".to_owned(),
                    visits: 1,
                    time: "2023-08-09T12:00:00Z".parse().unwrap(),
                }],
            ),
        ]
        .into_iter()
        .map(|(key, values)| (key.to_owned(), values))
        .collect();
        let operations = compact_operations(map).unwrap();
        assert_json_snapshot!(operations, @"[]");
    }

    #[test]
    fn test_compact_operations_two_even() {
        let map: HashMap<String, Vec<Analytics>> = [(
            "/test1",
            vec![
                Analytics {
                    id: uuid::uuid!("5357049c-585d-11ee-8592-f33eb664afbb"),
                    uri: "/test1".to_owned(),
                    visits: 1,
                    time: "2023-08-09T12:00:00Z".parse().unwrap(),
                },
                Analytics {
                    id: uuid::uuid!("6da48fa4-585d-11ee-a8f6-c73b3026321c"),
                    uri: "/test1".to_owned(),
                    visits: 1,
                    time: "2023-08-09T12:30:00Z".parse().unwrap(),
                },
            ],
        )]
        .into_iter()
        .map(|(key, values)| (key.to_owned(), values))
        .collect();
        let operations = compact_operations(map).unwrap();
        assert_json_snapshot!(operations, {
            "[].new.id" => "25841eaa-eabe-4895-a062-3516e5f4f530"
        },
        @r###"
        [
          {
            "delete": [
              {
                "id": "5357049c-585d-11ee-8592-f33eb664afbb",
                "uri": "/test1",
                "visits": 1,
                "time": "+002023-08-09T12:00:00.000000000Z"
              },
              {
                "id": "6da48fa4-585d-11ee-a8f6-c73b3026321c",
                "uri": "/test1",
                "visits": 1,
                "time": "+002023-08-09T12:30:00.000000000Z"
              }
            ],
            "new": {
              "id": "25841eaa-eabe-4895-a062-3516e5f4f530",
              "uri": "/test1",
              "visits": 2,
              "time": "+002023-08-09T12:15:00.000000000Z"
            }
          }
        ]
        "###);
    }

    #[test]
    fn test_compact_operations_two_weighted() {
        let map: HashMap<String, Vec<Analytics>> = [(
            "/test1",
            vec![
                Analytics {
                    id: uuid::uuid!("5357049c-585d-11ee-8592-f33eb664afbb"),
                    uri: "/test1".to_owned(),
                    visits: 1,
                    time: "2023-08-09T12:00:00Z".parse().unwrap(),
                },
                Analytics {
                    id: uuid::uuid!("6da48fa4-585d-11ee-a8f6-c73b3026321c"),
                    uri: "/test1".to_owned(),
                    visits: 2,
                    time: "2023-08-09T12:30:00Z".parse().unwrap(),
                },
            ],
        )]
        .into_iter()
        .map(|(key, values)| (key.to_owned(), values))
        .collect();
        let operations = compact_operations(map).unwrap();
        assert_json_snapshot!(operations, {
            "[].new.id" => "25841eaa-eabe-4895-a062-3516e5f4f530"
        }, 
        @r###"
        [
          {
            "delete": [
              {
                "id": "5357049c-585d-11ee-8592-f33eb664afbb",
                "uri": "/test1",
                "visits": 1,
                "time": "+002023-08-09T12:00:00.000000000Z"
              },
              {
                "id": "6da48fa4-585d-11ee-a8f6-c73b3026321c",
                "uri": "/test1",
                "visits": 2,
                "time": "+002023-08-09T12:30:00.000000000Z"
              }
            ],
            "new": {
              "id": "25841eaa-eabe-4895-a062-3516e5f4f530",
              "uri": "/test1",
              "visits": 3,
              "time": "+002023-08-09T12:20:00.000000000Z"
            }
          }
        ]
        "###);
    }
    fn analytics(uri: String) -> impl Strategy<Value = Analytics> {
        let start_time: types::Time = "1960-01-01T00:00:00Z".parse().unwrap();
        let end_time: types::Time = "2100-01-01T00:00:00Z".parse().unwrap();

        (
            start_time.unix_timestamp()..=end_time.unix_timestamp(),
            0..100u32,
        )
            .prop_map(move |(timestamp, visits)| {
                let time = OffsetDateTime::from_unix_timestamp(timestamp).unwrap();
                Analytics {
                    id: Uuid::new_v4(),
                    uri: uri.clone(),
                    visits,
                    time: time.into(),
                }
            })
    }

    fn map_entry() -> impl Strategy<Value = (String, Vec<Analytics>)> {
        proptest::string::string_regex("/key[0-9]")
            .expect("Error parsing map entry key regex")
            .prop_flat_map(|uri| {
                (
                    Just(uri.to_owned()),
                    proptest::collection::vec(analytics(uri.to_owned()), 0..10),
                )
            })
    }

    fn map() -> impl Strategy<Value = HashMap<String, Vec<Analytics>>> {
        proptest::collection::vec(map_entry(), 0..10)
            .prop_map(|entries| entries.into_iter().collect())
    }

    #[test]
    fn test_compact_operations_not_crash() {
        let mut runner = TestRunner::default();
        runner
            .run(&(map()), |map| {
                let expect_operations = map
                    .iter()
                    .find_map(|(_, value)| if value.len() > 1 { Some(()) } else { None })
                    .is_some();
                let operations = compact_operations(map).unwrap();

                if expect_operations {
                    assert!(!operations.is_empty());
                }

                for operation in operations {
                    assert!(operation
                        .delete
                        .iter()
                        .all(|delete| delete.uri == operation.new.uri));
                    let min_delete_time = operation
                        .delete
                        .iter()
                        .map(|delete| *delete.time)
                        .min()
                        .unwrap();
                    let max_delete_time = operation
                        .delete
                        .iter()
                        .map(|delete| *delete.time)
                        .max()
                        .unwrap();

                    assert!(min_delete_time <= *operation.new.time);
                    assert!(max_delete_time >= *operation.new.time);
                }
                Ok(())
            })
            .unwrap();
    }
}
