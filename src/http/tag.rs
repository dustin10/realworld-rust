use crate::http::{AppContext, Error};

use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// SQL query used to fetch tags from the database.
const LIST_TAGS_QUERY: &str = "SELECT * FROM tags";

/// Creates the [`Router`] for the HTTP endpoints that correspond to the `tag` domain and requires
/// the [`AppContext`] to be the state type.
///
/// The following list enumerates the endpoints which are exposed by the `tag` API.
///
/// * `GET /api/tags` - List the distinct tags that exist in the application.
pub(super) fn router() -> Router<AppContext> {
    Router::new().route("/api/tags", get(list_tags))
}

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

/// The [`TagsBody`] struct is the envelope in which the list of tag names that exist in the
/// application are returned to the client.
#[derive(Debug, Serialize)]
struct TagsBody {
    /// List of tag names.
    tags: Vec<String>,
}

/// Handles the list tags API endpoint at `GET /api/tags`.
///
/// # Response Body Format
///
/// ```json
/// {
///   "tags": [
///     "foo",
///     "bar"
///   ]
/// }
/// ```
async fn list_tags(ctx: State<AppContext>) -> Result<Json<TagsBody>, Error> {
    let tags = sqlx::query_as(LIST_TAGS_QUERY)
        .fetch_all(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?
        .into_iter()
        .map(|t: Tag| t.name)
        .collect();

    Ok(Json(TagsBody { tags }))
}
