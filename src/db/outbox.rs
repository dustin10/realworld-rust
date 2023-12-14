use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{types::Json, FromRow, PgConnection};
use std::collections::HashMap;
use uuid::Uuid;

/// SQL query used to create a new outbox entry in the database.
const CREATE_OUTBOX_ENTRY_QUERY: &str =
    "INSERT INTO outbox (topic, partition_key, headers, payload) VALUES ($1, $2, $3, $4) RETURNING *";

/// The [`OutboxEntry`] struct is used to let the `sqlx` library easily map a row from the `outbox`
/// table in the database to a struct value. It is a one-to-one mapping from the database table.
#[derive(Debug, FromRow)]
pub struct OutboxEntry {
    /// Id of the outbox entry.
    pub id: Uuid,
    /// Name of the Kafka topic to publish the event on.
    pub topic: String,
    /// Partition key for the event.
    pub partition_key: Option<String>,
    /// JSON representation of the event headers.
    pub headers: Option<Json<HashMap<String, String>>>,
    /// JSON representation of event data.
    pub payload: Option<String>,
    /// Time the outbox entry was created.
    pub created: DateTime<Utc>,
}

#[derive(Debug)]
pub struct CreateOutboxEntry<P: Serialize> {
    /// Name of the Kafka topic to publish the event on.
    pub topic: String,
    /// Partition key for the event.
    pub partition_key: Option<String>,
    /// Headers for the event.
    pub headers: Option<HashMap<String, String>>,
    /// Data that will be contained in the event.
    pub payload: Option<P>,
}

/// Inserts a new [`OutboxEntry`] row in the databa using the details contained in the specified
/// [`CreateOutboxEntry`].
pub async fn create_outbox_entry<P>(
    cxn: &mut PgConnection,
    entry: CreateOutboxEntry<P>,
) -> Result<OutboxEntry, sqlx::Error>
where
    P: Serialize + Send,
{
    // TODO: probably want to handle serialization error
    let payload_json = entry.payload.and_then(|p| serde_json::to_string(&p).ok());

    let headers_json = entry.headers.map(Json);

    sqlx::query_as(CREATE_OUTBOX_ENTRY_QUERY)
        .bind(entry.topic)
        .bind(entry.partition_key)
        .bind(headers_json)
        .bind(payload_json)
        .fetch_one(cxn)
        .await
}
