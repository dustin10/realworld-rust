use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// The [`Tag`] struct is a one-to-one mapping from the row in the database to a struct.
#[derive(Debug, FromRow)]
pub(crate) struct Tag {
    /// Id of the tag.
    #[allow(dead_code)]
    pub id: Uuid,
    /// Name of the tag.
    pub name: String,
    /// Time the tag was created.
    #[allow(dead_code)]
    pub created: DateTime<Utc>,
}
