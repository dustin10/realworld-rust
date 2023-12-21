use crate::{
    config::Config,
    db::{self, outbox::OutboxEntry},
    event::Error,
};

use rdkafka::{
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender};

/// Schedules a sweep of the outbox to process any entries that may have been missed when a message
/// was sent over the outbox channel.
pub async fn schedule_outbox_sweep(config: Arc<Config>, tx: Sender<()>) -> Result<(), Error> {
    let interval_ms = config.outbox.interval;

    tracing::info!("scheduling outbox entry sweep for every {}ms", interval_ms);

    let scheduled_task = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));

        loop {
            interval.tick().await;

            if let Err(e) = tx.send(()).await {
                tracing::error!("failed to notify outbox processor on schedule tick: {}", e);
            }
        }
    });

    // We should never really return here as we simply log when an error is encountered right now.
    Err(scheduled_task.await?)
}

/// Starts a task that receives messages on the given [`Receiver`] and processes a batch of outbox
/// entries when one is received.
pub async fn start_outbox_receiver(
    config: Arc<Config>,
    db: PgPool,
    mut rx: Receiver<()>,
) -> Result<(), Error> {
    // In a real production application the producer configuration would most likely need to be
    // more more finely tuned to meet the use case and performance requirements.
    let mut producer_config = rdkafka::ClientConfig::new();
    producer_config.set("bootstrap.servers", &config.kafka.servers);

    if tracing::enabled!(tracing::Level::DEBUG) {
        producer_config.set_log_level(rdkafka::config::RDKafkaLogLevel::Debug);
    }

    let producer: FutureProducer = producer_config.create()?;

    let batch_size = config.outbox.batch_size as i64;

    tracing::info!(
        "starting channel-based outbox receiver with batch size {}",
        batch_size,
    );

    let channel_task = tokio::task::spawn(async move {
        loop {
            if rx.recv().await.is_some() {
                match process_batch(&db, &producer, batch_size).await {
                    Err(e) => tracing::error!("error processing outbox batch: {}", e),
                    Ok(num_processed) => {
                        if num_processed > 0 {
                            tracing::info!("processed {} outbox entries", num_processed);
                        }
                    }
                }
            }
        }
    });

    // We should never really return here as we simply log when an error is encountered right now.
    Err(channel_task.await?)
}

/// Queries the database for a batch of outbox entries and then publishes an event to Kafka using the
/// details contained in the entry.
async fn process_batch(
    db: &PgPool,
    producer: &FutureProducer,
    batch_size: i64,
) -> Result<i64, Error> {
    let mut num_processed = 0;

    let mut tx = db.begin().await?;

    let batch = db::outbox::query_outbox_entry_batch(&mut tx, batch_size).await?;
    for entry in batch {
        process_entry(entry, producer).await?;
        num_processed += 1;
    }

    tx.commit().await?;

    Ok(num_processed)
}

/// Transforms the [`OutboxEntry`] into a Kafka record and publishes it onto the appropriate topic.
async fn process_entry(entry: OutboxEntry, producer: &FutureProducer) -> Result<(), Error> {
    let mut headers = OwnedHeaders::new();
    if let Some(entry_headers) = entry.headers {
        for (k, v) in entry_headers.0 {
            headers = headers.insert(Header {
                key: &k,
                value: Some(&v),
            });
        }
    }

    let mut record = FutureRecord::to(&entry.topic).headers(headers);

    if let Some(pk) = &entry.partition_key {
        record = record.key(pk);
    }

    if let Some(p) = &entry.payload {
        record = record.payload(p);
    }

    producer
        .send(record, Timeout::After(Duration::from_secs(5)))
        .await
        .map(|(p, o)| {
            tracing::debug!(
                "published event to topic {} on partition {} at offset {}",
                &entry.topic,
                &p,
                &o
            )
        })
        .map_err(|e| e.0.into())
}
