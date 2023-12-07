use crate::{
    db,
    http::{AppContext, Error},
};

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

/// Creates the [`Router`] for the HTTP endpoints that correspond to the `tag` domain and requires
/// the [`AppContext`] to be the state type.
///
/// The following list enumerates the endpoints which are exposed by the `tag` API.
///
/// * `GET /api/tags` - List the distinct tags that exist in the application.
pub(super) fn router() -> Router<AppContext> {
    Router::new().route("/api/tags", get(list_tags))
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
    let tags = db::tag::fetch_all_tags(&ctx.db)
        .await?
        .into_iter()
        .map(|t: db::tag::Tag| t.name)
        .collect();

    Ok(Json(TagsBody { tags }))
}
