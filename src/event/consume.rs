use crate::{config::Config, event::Error};

use rdkafka::consumer::StreamConsumer;
use std::sync::Arc;

/// Initialize the Kafka consumer from the application configuration.
pub async fn start_kafka_consumer(config: Arc<Config>) -> Result<(), Error> {
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
