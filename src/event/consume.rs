use crate::{config::Config, event::Error};

use futures::TryStreamExt;
use rdkafka::{
    consumer::{Consumer, ConsumerContext, Rebalance, StreamConsumer},
    error::KafkaResult,
    message::{BorrowedMessage, Headers},
    ClientContext, Message, Statistics, TopicPartitionList,
};
use std::sync::Arc;

/// The [`ConsumeContext`] is a struct that is used to implement a custom Kafka consumer context to
/// hook into key events in the lifecycle of a Kafka consumer.
struct ConsumeContext;

impl ClientContext for ConsumeContext {
    /// Hook invoked when that statistics are gathered for the consumer group at the configured
    /// interval.
    fn stats(&self, statistics: Statistics) {
        tracing::debug!("consumer statistics received: {:?}", statistics);
    }
}

impl ConsumerContext for ConsumeContext {
    /// Hook invoked right before the consumer begins rebalancing.
    fn pre_rebalance(&self, rebalance: &Rebalance) {
        tracing::info!("topic rebalance initiated: {:?}", rebalance);
    }
    /// Hook invoked after the consumer rebalancing has been completed.
    fn post_rebalance(&self, rebalance: &Rebalance) {
        match rebalance {
            Rebalance::Assign(tpl) => {
                tpl.elements().iter().for_each(|e| {
                    tracing::info!("partition {} on {} assigned", e.partition(), e.topic())
                });
            }
            Rebalance::Revoke(tpl) => {
                tpl.elements().iter().for_each(|e| {
                    tracing::info!("partition {} on {} revoked", e.partition(), e.topic())
                });
            }
            Rebalance::Error(err) => tracing::error!("error during topic rebalance: {}", err),
        }
    }
    /// Hook invoked after the consumer has attempted to commit offsets.
    fn commit_callback(&self, result: KafkaResult<()>, offsets: &TopicPartitionList) {
        match result {
            Ok(_) => {
                if tracing::event_enabled!(tracing::Level::DEBUG) {
                    offsets.elements().iter().for_each(|e| {
                        tracing::debug!(
                            "committed offset {:?} on partition {} in topic {}",
                            e.offset(),
                            e.partition(),
                            e.topic()
                        )
                    });
                }
            }
            Err(e) => {
                tracing::error!("error committing Kafka consumer offsets: {}", e);
            }
        }
    }
}

/// Starts the Kafka consumer configured with the application configuration.
pub async fn start_kafka_consumer(config: Arc<Config>) -> Result<(), Error> {
    // Similar to the producer, in a real production application the configuration would need to
    // be tuned to best meet the use case and performance requirements of the application. For
    // instance, you would most likely want to manage the committing offsets yourself rather than
    // having auto commit enabled.
    let mut consumer_config = rdkafka::ClientConfig::new();
    consumer_config.set("group.id", "realworld");
    consumer_config.set("bootstrap.servers", &config.kafka.servers);
    consumer_config.set("enable.auto.commit", "true");
    consumer_config.set("statistics.interval.ms", "120000");
    consumer_config.set("auto.offset.reset", "latest");

    if tracing::enabled!(tracing::Level::DEBUG) {
        consumer_config.set_log_level(rdkafka::config::RDKafkaLogLevel::Debug);
    }

    let consumer: StreamConsumer<ConsumeContext> =
        consumer_config.create_with_context(ConsumeContext)?;

    consumer.subscribe(&["article", "user"])?;

    let stream_processor = consumer
        .stream()
        .try_for_each(|msg: BorrowedMessage| async move {
            // Here you could do any processing you need to on the messages that you recieve. This
            // consumer will be subscribed to both the `article` and `user` topics and simply print
            // out the payload that is received. A lot of this depends on how your topics and
            // events are laid out but for this application how the event was processed would be
            // determined by the topic the event was received on and `type` header value.

            // Extract event type header value.
            let mut event_type = "unknown";
            if let Some(headers) = msg.headers() {
                for (idx, header) in headers.iter().enumerate() {
                    if header.key == "type" {
                        event_type = headers
                            .try_get_as(idx)
                            .and_then(|h| h.ok())
                            .and_then(|h| h.value)
                            .unwrap_or(event_type);
                    }
                }
            }

            // Log appropriate message based on the message payload.
            match msg.payload_view::<str>() {
                Some(Ok(payload)) => {
                    tracing::info!(
                        "received event of type {} on {} with payload: {}",
                        event_type,
                        msg.topic(),
                        payload
                    );
                }
                Some(Err(err)) => {
                    tracing::error!(
                        "received event of type {} on {} with invalid string payload: {}",
                        event_type,
                        msg.topic(),
                        err
                    )
                }
                None => tracing::info!(
                    "received event of type {} on {} with no payload",
                    event_type,
                    msg.topic()
                ),
            }

            Ok(())
        });

    stream_processor.await.map_err(|e| e.into())
}
