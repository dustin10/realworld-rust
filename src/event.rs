use crate::{
    config::Config,
    db::{self, outbox::OutboxEntry},
};

use rdkafka::{
    consumer::StreamConsumer,
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("error initializing Kafka")]
    Initialization {
        #[from]
        source: rdkafka::error::KafkaError,
    },
    #[error("error processing outbox entries")]
    OutboxProcessing {
        #[from]
        source: tokio::task::JoinError,
    },
    #[error("error querying the database for outbox entries")]
    OutboxDatabase {
        #[from]
        source: sqlx::Error,
    },
    #[error("error publishing Kafka event for an outbox entry")]
    OutboxPublish,
    #[error("error consuming Kafka events")]
    EventProcessing,
}

/// Starts the outbox processing task that will execute at the configured interval and process
/// any entries in the `outbox` database table by submitting the corresponding event to Kafka.
pub async fn start_outbox_processor(db: PgPool, config: Arc<Config>) -> Result<(), Error> {
    // In a real production application the producer configuration would need to much more more
    // finely tuned to meet the use case and performance requirements.
    let mut producer_config = rdkafka::ClientConfig::new();
    producer_config.set("bootstrap.servers", &config.kafka.servers);

    if tracing::enabled!(tracing::Level::DEBUG) {
        producer_config.set_log_level(rdkafka::config::RDKafkaLogLevel::Debug);
    }

    let producer: FutureProducer = producer_config.create()?;

    let batch_size = config.outbox.batch_size as i64;

    let task = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(config.outbox.interval));

        loop {
            interval.tick().await;

            match process_batch(&db, &producer, batch_size).await {
                Err(e) => return e,
                Ok(num_processed) => {
                    if num_processed > 0 {
                        tracing::info!("processed {} outbox entries", num_processed);
                    }
                }
            }
        }
    });

    // We should never get here unless an unexpected error occurred while processing the outbox
    // entries. In that case we go ahead and return the error and shutdown the application.
    Err(task.await?)
}

/// Queries the database for a batch of outbox entries and then publish events to Kafka using the
/// details contained in the entry.
async fn process_batch(
    db: &PgPool,
    producer: &FutureProducer,
    batch_size: i64,
) -> Result<i64, Error> {
    let mut num_processed = 0;

    let mut cxn = db.acquire().await?;

    let batch = db::outbox::query_outbox_entry_batch(&mut cxn, batch_size).await?;
    for entry in batch {
        process_entry(entry, producer).await?;
        num_processed += 1;
    }

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
        .map_err(|e| {
            tracing::error!("error publishing to Kafka: {}", e.0);
            Error::OutboxPublish
        })
}

/// Initialize the Kafka consumer from the application configuration.
pub async fn init_kafka_consumer(config: Arc<Config>) -> Result<(), Error> {
    // Similar to the producer, in a real production application the configuration would need to
    // be tuned to best meet the use case and performance requirements of the application.
    let mut consumer_config = rdkafka::ClientConfig::new();
    consumer_config.set("group.id", "realworld");
    consumer_config.set("bootstrap.servers", &config.kafka.servers);
    consumer_config.set("enable.auto.commit", "false");
    consumer_config.set("statistics.interval.ms", "120000");
    consumer_config.set("auto.offset.reset", "latest");

    if tracing::enabled!(tracing::Level::DEBUG) {
        consumer_config.set_log_level(rdkafka::config::RDKafkaLogLevel::Debug);
    }

    let _consumer: StreamConsumer = consumer_config.create()?;

    Ok(())
}
