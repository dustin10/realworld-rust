use crate::http::{
    auth::AuthContext,
    profile::{self, Profile},
    tag, AppContext, Error,
};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to create a new article.
const CREATE_ARTICLE_QUERY: &str =
    "INSERT INTO articles (user_id, slug, title, description, body) VALUES ($1, $2, $3, $4, $5) RETURNING *";

/// SQL query used to fetch an article by slug.
const GET_ARTICLE_BY_SLUG_QUERY: &str = "SELECT * FROM articles WHERE slug = $1";

/// SQL query used to fetch a computed view of an article by slug.
const GET_ARTICLE_VIEW_BY_SLUG_QUERY: &str = r#"
    SELECT
        a.*,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id AND af.user_id = $1)::int::bool AS favorited,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id) as favorites_count
    FROM
        articles AS a
    WHERE
        a.slug = $2"#;

/// SQL query used to delete entries from the user favorites join table for an article.
const DELETE_ARTICLE_FAVS_QUERY: &str = "DELETE FROM article_favs WHERE article_id = $1";

/// SQL query used to delete an article.
const DELETE_ARTICLE_QUERY: &str = "DELETE FROM articles WHERE id = $1";

/// Creates the [`Router`] for the HTTP endpoints that correspond to the `article` domain and requires
/// the [`AppContext`] to be the state type.
///
/// The following list enumerates the endpoints which are exposed by the `article` API.
///
/// * `GET /api/articles` - List multiple articles with filters ordered by the most recent first.
/// * `GET /api/articles/feed` - Authentication required, will return multiple articles created by followed
/// users, ordered by most recent first.
/// * `GET /api/articles/:slug` - Returns a single article.
/// * `POST /api/articles` - Authentication required, creates a new article.
/// * `PUT /api/articles/:slug` - Authentication required, updates an existing article.
/// * `DELETE /api/articles/:slug` - Authentication required, deletes an existing article.
/// * `POST /api/articles/:slug/comments` - Authentication required, creates a new comment on an
/// article.
/// * `GET /api/articles/:slug/comments` - Lists all comments for an article.
/// * `DELETE /api/articles/:slug/comments/:id` - Authentication required, deletes a comment on an
/// article.
/// * `POST /api/articles/:slug/favorite` - Authentication required, favorites an article.
/// * `DELETE /api/articles/:slug/favorite` - Authentication required, removes an article from
/// favorites.
pub(super) fn router() -> Router<AppContext> {
    Router::new()
        .route("/api/articles", post(create_article))
        .route(
            "/api/articles/:slug",
            get(get_article).delete(delete_article),
        )
}

/// The [`UserRow`] struct is used to let the `sqlx` library easily map a row from the `users` table
/// in the database to a struct value. It is a one-to-one mapping from the database table.
#[derive(Debug, FromRow)]
struct ArticleRow {
    /// Id of the article.
    id: Uuid,
    /// Id of the user who authored the article.
    user_id: Uuid,
    /// Slugified title of the article.
    slug: String,
    /// Title of the article.
    #[allow(dead_code)]
    title: String,
    /// Description of the article.
    #[allow(dead_code)]
    description: String,
    /// Body of the article.
    #[allow(dead_code)]
    body: String,
    /// Time the article was created.
    #[allow(dead_code)]
    created: DateTime<Utc>,
    /// Time the article was last modified.
    #[allow(dead_code)]
    updated: Option<DateTime<Utc>>,
}

/// The [`ArticleView`] struct is used to let the `sqlx` library easily map a view of the `articles`
/// table and supporting data in the database to a struct value. This is not a one-to-one mapping
/// from the row to the struct but rather there is also some computed data returned. Hence, why the
/// name is a view and not a row. Some may also refer to this as a projection.
#[derive(Debug, FromRow)]
struct ArticleView {
    /// Id of the article.
    id: Uuid,
    /// Id of the article.
    user_id: Uuid,
    /// Slugified title of the article.
    slug: String,
    /// Title of the article.
    title: String,
    /// Description of the article.
    description: String,
    /// Body of the article.
    body: String,
    /// Time the article was created.
    created: DateTime<Utc>,
    /// Time the article was last modified.
    updated: Option<DateTime<Utc>>,
    /// Flag indicating whether the logged in user, if available, has favorited the article.
    favorited: bool,
    /// Count of the total number of users who have favorited the article.
    favorites_count: i64,
}

/// The [`Article`] struct contains data that repesents an article as returned from the API.
#[derive(Debug, Serialize)]
struct Article {
    /// Slugified title of the article.
    slug: String,
    /// Title of the article.
    title: String,
    /// Description of the article.
    description: String,
    /// Body of the article.
    body: String,
    /// List of tags associated with the article.
    #[serde(rename = "tagList")]
    tags: Option<Vec<String>>,
    /// Time the article was created.
    #[serde(rename = "createdAt")]
    created: DateTime<Utc>,
    /// Time the article was last modified.
    #[serde(rename = "updatedAt")]
    updated: Option<DateTime<Utc>>,
    /// Flag indicating whether the logged in user, if available, has favorited the article.
    favorited: bool,
    /// Count of the total number of users who have favorited the article.
    #[serde(rename = "favoritesCount")]
    favorites_count: i64,
    /// Public [`Profile`] of the user who authored the article.
    author: Profile,
}

/// The [`ArticleBody`] struct is the envelope in which different data for an article is
/// returned to the client or accepted from the client.
#[derive(Debug, Deserialize, Serialize)]
struct ArticleBody<T> {
    /// Article related data.
    article: T,
}

/// The [`CreateArticle`] struct contains the data received from the HTTP request to create a new
/// article.
#[derive(Debug, Deserialize, Serialize)]
struct CreateArticle {
    /// Title of the article.
    title: String,
    /// Description of the article.
    description: String,
    /// Body of the article.
    body: String,
    /// List of tags associated with the article.
    #[serde(rename = "tagList")]
    tags: Option<Vec<String>>,
}

/// Handles the create article API endpoint at `POST /api/articles`.
///
/// # Request Body Format
///
/// ``` json
/// {
///   "article":{
///     "title": "How to train your dragon",
///     "description": "Ever wonder how?",
///     "body": "You have to believe",
///     "tagList": ["reactjs", "angularjs", "dragons"]
///   }
/// }
/// ```
///
/// # Field Validation
///
/// * `title` - required
/// * `description` - required
/// * `body` - required
/// * `tagList` - optional
///
/// # Response Body Format
///
/// ```json
/// {
///   "article": {
///     "slug": "how-to-train-your-dragon",
///     "title": "How to train your dragon",
///     "description": "Ever wonder how?",
///     "body": "It takes a Jacobian",
///     "tagList": ["dragons", "training"],
///     "createdAt": "2016-02-18T03:22:56.637Z",
///     "updatedAt": "2016-02-18T03:48:35.824Z",
///     "favorited": false,
///     "favoritesCount": 0,
///     "author": {
///       "username": "jake",
///       "bio": "I work at statefarm",
///       "image": "https://i.stack.imgur.com/xHWG8.jpg",
///       "following": false
///     }
///   }
/// }
/// ```
async fn create_article(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Json(request): Json<ArticleBody<CreateArticle>>,
) -> Result<Response, Error> {
    let row = insert_article(&ctx.db, &auth_ctx.user_id, &request.article).await?;

    match fetch_article(&ctx.db, &row.slug, None).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => Ok(Json(ArticleBody { article }).into_response()),
    }
}

/// Handles the get article by slug API endpoint at `GET /api/articles/:slug`. The handler will
/// read the `slug` path parameter value and return the data for the matching article if it exists,
/// otherwise it will return a 404 response.
///
/// If the request is authenticated, then the favorited and following metadata properties property
/// of the response will indicate whether the currently authenticated user is following the profile
/// of the article author and also whether the article has been favorited by the user.
///
/// If the request is made unauthenticated, then the favorited and following metadata will always
/// be set to `false`.
///
/// # Response Body Format
///
/// ```json
/// {
///   "article": {
///     "slug": "how-to-train-your-dragon",
///     "title": "How to train your dragon",
///     "description": "Ever wonder how?",
///     "body": "It takes a Jacobian",
///     "tagList": ["dragons", "training"],
///     "createdAt": "2016-02-18T03:22:56.637Z",
///     "updatedAt": "2016-02-18T03:48:35.824Z",
///     "favorited": false,
///     "favoritesCount": 0,
///     "author": {
///       "username": "jake",
///       "bio": "I work at statefarm",
///       "image": "https://i.stack.imgur.com/xHWG8.jpg",
///       "following": false
///     }
///   }
/// }
/// ```
async fn get_article(
    ctx: State<AppContext>,
    auth_ctx: Option<AuthContext>,
    Path(slug): Path<String>,
) -> Result<Response, Error> {
    let user_ctx = auth_ctx.map(|ac| ac.user_id);

    match fetch_article(&ctx.db, &slug, user_ctx).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => Ok(Json(ArticleBody { article }).into_response()),
    }
}

/// Handles the delete article by slug API endpoint at `DELETE /api/articles/:slug`. The handler
/// will read the `slug` path parameter value and delete the article and all associated data for
/// the matching article if it exists and the authenticated user is the author. Otherwise a 404
/// respone will be returned.
async fn delete_article(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path(slug): Path<String>,
) -> Result<Response, Error> {
    // TODO: transaction
    match fetch_article_by_slug(&ctx.db, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            if auth_ctx.user_id != article.user_id {
                return Ok(StatusCode::FORBIDDEN.into_response());
            }

            delete_article_favs(&ctx.db, &article.id).await?;
            tag::delete_article_tags(&ctx.db, &article.id).await?;
            delete_article_by_id(&ctx.db, &article.id).await?;

            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}

/// Retrieves the [`Article`] from the database by slug if it exists, with the specified user
/// context.
async fn fetch_article(
    db: &PgPool,
    slug: &str,
    user_ctx: Option<Uuid>,
) -> Result<Option<Article>, Error> {
    match fetch_article_view_by_slug(db, slug, user_ctx).await? {
        None => Ok(None),
        Some(view) => {
            let tags = tag::fetch_tags_for_article(db, &view.id)
                .await?
                .into_iter()
                .map(|t| t.name)
                .collect();

            let author = profile::fetch_profile_by_id(db, &view.user_id, user_ctx)
                .await?
                .expect("authenticated user exists");

            // TODO: function or builder?
            let article = Article {
                slug: view.slug,
                title: view.title,
                description: view.description,
                body: view.body,
                created: view.created,
                updated: view.updated,
                favorited: view.favorited,
                favorites_count: view.favorites_count,
                tags: Some(tags),
                author,
            };

            Ok(Some(article))
        }
    }
}

/// Inserts the article as well as any tags and their assocations into the database. Returns the
/// [`ArticleRow`] that was created in the database.
async fn insert_article(
    db: &PgPool,
    user_id: &Uuid,
    article: &CreateArticle,
) -> Result<ArticleRow, Error> {
    let slug = slug::slugify(&article.title);

    let row: ArticleRow = sqlx::query_as(CREATE_ARTICLE_QUERY)
        .bind(user_id)
        .bind(slug)
        .bind(&article.title)
        .bind(&article.description)
        .bind(&article.body)
        .fetch_one(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    if let Some(tags) = &article.tags {
        // TODO: could probably be more efficient
        for name in tags {
            let tag = tag::insert_tag(db, name).await?;

            tag::insert_article_tag(db, &row.id, &tag.id).await?;
        }
    }

    Ok(row)
}

async fn fetch_article_by_slug(db: &PgPool, slug: &str) -> Result<Option<ArticleRow>, Error> {
    sqlx::query_as(GET_ARTICLE_BY_SLUG_QUERY)
        .bind(slug)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Retrieves an [`ArticleView`] identified by the specified slug using the identifier of the
/// authenticated user, if available, as the user context to determine if the article is favorited
/// or not.
async fn fetch_article_view_by_slug(
    db: &PgPool,
    slug: &str,
    auth_id: Option<Uuid>,
) -> Result<Option<ArticleView>, Error> {
    let user_context = auth_id.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_ARTICLE_VIEW_BY_SLUG_QUERY)
        .bind(user_context)
        .bind(slug)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

async fn delete_article_favs(db: &PgPool, article_id: &Uuid) -> Result<(), Error> {
    let _ = sqlx::query(DELETE_ARTICLE_FAVS_QUERY)
        .bind(article_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    Ok(())
}

async fn delete_article_by_id(db: &PgPool, article_id: &Uuid) -> Result<(), Error> {
    let _ = sqlx::query(DELETE_ARTICLE_QUERY)
        .bind(article_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    Ok(())
}
