use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch tags from the database.
const LIST_TAGS_QUERY: &str = "SELECT * FROM tags";

/// The [`Tag`] struct is a one-to-one mapping from the row in the database to a struct.
#[derive(Debug, FromRow)]
pub struct Tag {
    /// Id of the tag.
    #[allow(dead_code)]
    pub id: Uuid,
    /// Name of the tag.
    pub name: String,
    /// Time the tag was created.
    #[allow(dead_code)]
    pub created: DateTime<Utc>,
}

/// Queries the database for all existing [`Tag`]s and returns them in a [`Vec`]. The API spec for
/// the application does not call for any paging or filtering here but that would probably be more
/// appropriate in a real production application. For instance, you may want to query for the most
/// used tags, etc.
pub async fn fetch_all_tags(db: &PgPool) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as(LIST_TAGS_QUERY).fetch_all(db).await
}
