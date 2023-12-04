use crate::{
    db::tag::Tag,
    http::{
        auth::AuthContext,
        profile::{self, Profile},
        AppContext, Error, Pagination,
    },
};

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch a single page of the article feed for a user.
const GET_USER_FEED_PAGE_QUERY: &str = r#"
    SELECT 
        a.*,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id AND af.user_id = $1)::int::bool AS favorited,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id) as favorites_count
    FROM articles AS a INNER JOIN user_follows AS uf ON a.user_id = uf.user_id
    WHERE uf.follower_id = $1
    ORDER BY a.created DESC
    LIMIT $2
    OFFSET $3"#;

/// SQL query used to create a new article.
const CREATE_ARTICLE_QUERY: &str =
    "INSERT INTO articles (user_id, slug, title, description, body) VALUES ($1, $2, $3, $4, $5) RETURNING *";

/// SQL query used to create a new tag in the database.
const CREATE_TAG_QUERY: &str = r#"
    INSERT INTO
        tags (name)
    VALUES
        ($1)
    ON CONFLICT(name) DO UPDATE SET name = EXCLUDED.name
    RETURNING *"#;

/// SQL query used to create the association of a tag to an article.
const CREATE_ARTICLE_TAG_QUERY: &str =
    "INSERT INTO article_tags (article_id, tag_id) VALUES ($1, $2)";

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

/// SQL query used to fetch the tags for an article from the database.
const GET_TAGS_FOR_ARTICLE_QUERY: &str = r#"
    SELECT
        t.*
    FROM
        tags AS t INNER JOIN article_tags AS at ON t.id = at.tag_id
    WHERE
        at.article_id = $1"#;

/// SQL query used to delete entries from the user favorites join table for an article.
const DELETE_ARTICLE_FAVS_QUERY: &str = "DELETE FROM article_favs WHERE article_id = $1";

/// SQL query used to delete the links from a tag to an article.
const DELETE_ARTICLE_TAGS_QUERY: &str = "DELETE FROM article_tags WHERE article_id = $1";

/// SQL query used to delete an article.
const DELETE_ARTICLE_QUERY: &str = "DELETE FROM articles WHERE id = $1";

/// SQL query used to create a new comment for an article.
const CREATE_ARTICLE_COMMENT_QUERY: &str =
    "INSERT INTO article_comments (user_id, article_id, body) VALUES ($1, $2, $3) RETURNING *";

/// SQL query used to fetch the comments for a single article.
const GET_ARTICLE_COMMENTS_QUERY: &str = r#"
    SELECT ac.* 
    FROM article_comments AS ac INNER JOIN articles AS a ON ac.article_id = a.id
    WHERE a.slug = $1
    ORDER BY ac.created ASC"#;

/// SQL query used to delete a comment from an article.
const DELETE_ARTICLE_COMMENT_QUERY: &str =
    "DELETE FROM article_comments WHERE id = $1 AND user_id = $2";

/// SQL query used to create an entry in the table that captures favorited articles for a user.
const CREATE_USER_ARTICLE_FAV_QUERY: &str =
    "INSERT INTO article_favs (article_id, user_id) VALUES ($1, $2) RETURNING *";

/// SQL query used to delete the entry in the table that captures favorited articles for a user.
const DELETE_USER_ARTICLE_FAV_QUERY: &str =
    "DELETE FROM article_favs WHERE article_id = $1 AND user_id = $2";

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
        .route("/api/articles/feed", get(user_feed))
        .route("/api/articles", post(create_article))
        .route(
            "/api/articles/:slug",
            get(get_article).delete(delete_article),
        )
        .route(
            "/api/articles/:slug/favorite",
            post(favorite_article).delete(unfavorite_article),
        )
        .route(
            "/api/articles/:slug/comments",
            post(create_comment).get(get_comments),
        )
        .route("/api/articles/:slug/comments/:id", delete(delete_comment))
}

/// The [`ArticleRow`] struct is used to let the `sqlx` library easily map a row from the `articles`
/// table in the database to a struct value. It is a one-to-one mapping from the database table.
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

/// The [`Article`] struct contains data that repesents an article as returned from the API. It
/// contains the relevant article data, tag data and properties relevant to the currently
/// authenticted user if one exists.
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

impl Article {
    /// Creates a new [`Article`] with the given supporting data.
    fn with_view_tags_and_profile(view: ArticleView, tags: Vec<String>, profile: Profile) -> Self {
        Article {
            slug: view.slug,
            title: view.title,
            description: view.description,
            body: view.body,
            created: view.created,
            updated: view.updated,
            favorited: view.favorited,
            favorites_count: view.favorites_count,
            tags: Some(tags),
            author: profile,
        }
    }
}

/// The [`ArticleBody`] struct is the envelope in which different data for an article is
/// returned to the client or accepted from the client.
#[derive(Debug, Deserialize, Serialize)]
struct ArticleBody<T> {
    /// Article related data.
    article: T,
}

/// The [`ArticlesBody`] struct is the envelope in which multiple [`Article`]s are returned to the
/// client.
#[derive(Debug, Serialize)]
struct ArticlesBody {
    /// Articles that make up the response body.
    articles: Vec<Article>,
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

/// The [`CommentBody`] struct is the envelope in which data for a comment is returned to the
/// client based on the incoming request.
#[derive(Debug, Deserialize, Serialize)]
struct CommentBody<T> {
    /// Comment data contained in the envelope.
    comment: T,
}

/// The [`CommentsBody`] struct is the envelope in which multiple [`Comments`]s for a given article
/// are returned to the client.
#[derive(Debug, Serialize)]
struct CommentsBody {
    /// [`Vec`] of [`Comment`]s for an article.
    comments: Vec<Comment>,
}

/// The [`CreateComment`] struct contains the data received from the HTTP request to create a new
/// comment on an article.
#[derive(Debug, Deserialize)]
struct CreateComment {
    /// Text of the comment.
    body: String,
}

/// The [`CommentRow`] struct is used to let the `sqlx` library easily map a row from the
/// `comments` table in the database to a struct value.
#[derive(Debug, FromRow)]
struct CommentRow {
    /// Id of the comment.
    id: Uuid,
    /// Id of the user who authored the comment.
    user_id: Uuid,
    /// Id of the article the comment was made on.
    #[allow(dead_code)]
    article_id: Uuid,
    /// Body text of the comment.
    body: String,
    /// Time at which the comment was made.
    created: DateTime<Utc>,
}

/// The [`Comment`] struct contains data that repesents a comment on an article made by a
/// registered user of the application.
#[derive(Debug, Serialize)]
struct Comment {
    /// Id of the comment.
    id: Uuid,
    /// Body text of the comment.
    body: String,
    /// Time at which the comment was made.
    #[serde(rename = "createdAt")]
    created: DateTime<Utc>,
    /// Public profile of the user who made the comment.
    author: Profile,
}

impl Comment {
    /// Creates a new [`Comment`] given the database row and author profile.
    fn from_row_and_profile(row: CommentRow, profile: Profile) -> Self {
        Self {
            id: row.id,
            body: row.body,
            created: row.created,
            author: profile,
        }
    }
}

/// Handles the get user feed endpoint at `GET /api/articles/feed` which returns articles authored
/// by users who the currently authenticted user follows.
///
/// # Response Body Format
///
/// ```json
/// {
///   "articles": [{
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
///   }]
/// }
/// ```
async fn user_feed(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    page: Query<Pagination>,
) -> Result<Json<ArticlesBody>, Error> {
    let article_views =
        fetch_article_views_for_user_feed(&ctx.db, &auth_ctx.user_id, &page.0).await?;

    let mut articles = Vec::with_capacity(article_views.len());

    for view in article_views {
        let tags = sqlx::query_as(GET_TAGS_FOR_ARTICLE_QUERY)
            .bind(view.id)
            .fetch_all(&ctx.db)
            .await
            .map_err(|e| {
                tracing::error!("error returned from the database: {}", e);
                Error::from(e)
            })?
            .into_iter()
            .map(|t: Tag| t.name)
            .collect();

        let author = profile::fetch_profile_by_id(&ctx.db, &view.user_id, Some(auth_ctx.user_id))
            .await?
            .expect("article author exists");

        let article = Article::with_view_tags_and_profile(view, tags, author);

        articles.push(article);
    }

    Ok(Json(ArticlesBody { articles }))
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
/// the matching article if it exists and the authenticated user is the author. If the article does
/// not exist then a 404 will be returned. If the authenticated user is not the author of the
/// article then a 403 response will be returned.
async fn delete_article(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path(slug): Path<String>,
) -> Result<Response, Error> {
    match fetch_article_row_by_slug(&ctx.db, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            if auth_ctx.user_id != article.user_id {
                return Ok(StatusCode::FORBIDDEN.into_response());
            }

            let mut tx = ctx.db.begin().await?;

            // delete any favorites
            let _ = sqlx::query(DELETE_ARTICLE_FAVS_QUERY)
                .bind(article.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            // delete any tags associations
            let _ = sqlx::query(DELETE_ARTICLE_TAGS_QUERY)
                .bind(article.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            // finally delete the article
            let _ = sqlx::query(DELETE_ARTICLE_QUERY)
                .bind(article.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            tx.commit().await?;

            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}

/// Handles the create article comment API endpoint at `POST /api/articles/:slug/comments`.
///
/// # Request Body Format
///
/// ``` json
/// {
///   "comment":{
///     "body": "His name was my name too."
///   }
/// }
/// ```
///
/// # Field Validation
///
/// * `body` - required
///
/// # Response Body Format
///
/// ```json
/// {
///   "comment": {
///     "id": 1,
///     "createdAt": "2016-02-18T03:22:56.637Z",
///     "body": "It takes a Jacobian",
///     "author": {
///       "username": "jake",
///       "bio": "I work at statefarm",
///       "image": "https://i.stack.imgur.com/xHWG8.jpg",
///       "following": false
///     }
///   }
/// }
/// ```
async fn create_comment(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path(slug): Path<String>,
    Json(request): Json<CommentBody<CreateComment>>,
) -> Result<Response, Error> {
    match fetch_article_row_by_slug(&ctx.db, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(row) => {
            let comment_row: CommentRow = sqlx::query_as(CREATE_ARTICLE_COMMENT_QUERY)
                .bind(auth_ctx.user_id)
                .bind(row.id)
                .bind(&request.comment.body)
                .fetch_one(&ctx.db)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            let profile =
                profile::fetch_profile_by_id(&ctx.db, &comment_row.user_id, Some(auth_ctx.user_id))
                    .await?
                    .expect("comment author should exist");

            let comment = Comment::from_row_and_profile(comment_row, profile);

            Ok(Json(CommentBody { comment }).into_response())
        }
    }
}

/// Handles the get article comments API endpoint at `GET /api/articles/:slug/comments`. If there
/// is an authentication context associated with the request then the comment author's profile will
/// be populated based on the authenticated user.
///
/// # Response Body Format
///
/// ```json
/// {
///   "comments": [{
///     "id": 1,
///     "createdAt": "2016-02-18T03:22:56.637Z",
///     "body": "It takes a Jacobian",
///     "author": {
///       "username": "jake",
///       "bio": "I work at statefarm",
///       "image": "https://i.stack.imgur.com/xHWG8.jpg",
///       "following": false
///     }
///   }]
/// }
/// ```
async fn get_comments(
    ctx: State<AppContext>,
    auth_ctx: Option<AuthContext>,
    Path(slug): Path<String>,
) -> Result<Json<CommentsBody>, Error> {
    let comment_rows: Vec<CommentRow> = sqlx::query_as(GET_ARTICLE_COMMENTS_QUERY)
        .bind(slug)
        .fetch_all(&ctx.db)
        .await?;

    let user_context = auth_ctx.map(|ac| ac.user_id);

    let mut comments = Vec::with_capacity(comment_rows.len());

    for row in comment_rows {
        let profile = profile::fetch_profile_by_id(&ctx.db, &row.user_id, user_context)
            .await?
            .expect("comment author should exist");

        let comment = Comment::from_row_and_profile(row, profile);

        comments.push(comment);
    }

    Ok(Json(CommentsBody { comments }))
}

/// Handles the delete article comment API endpoint at `DELETE /api/articles/:slug/comments/:id`.
async fn delete_comment(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path((_slug, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, Error> {
    // TODO: we could do better here by checking affected rows affected and returning 404 if zero

    sqlx::query(DELETE_ARTICLE_COMMENT_QUERY)
        .bind(id)
        .bind(auth_ctx.user_id)
        .execute(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Handles the favorite article API endpoint at `POST /api/articles/:slug/favorite`. The handler
/// will read the `slug` path parameter value, favorite the article using the currently authenticated
/// user and return the data for the matching article if it exists, otherwise it will return a 404
/// response.
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
async fn favorite_article(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path(slug): Path<String>,
) -> Result<Response, Error> {
    // TODO: handle case where favorite entry already exists

    match fetch_article_row_by_slug(&ctx.db, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(row) => {
            sqlx::query(CREATE_USER_ARTICLE_FAV_QUERY)
                .bind(row.id)
                .bind(auth_ctx.user_id)
                .execute(&ctx.db)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            let article = fetch_article(&ctx.db, &slug, Some(auth_ctx.user_id))
                .await?
                .expect("article exists");

            Ok(Json(ArticleBody { article }).into_response())
        }
    }
}

/// Handles the unfavorite article API endpoint at `DELETE /api/articles/:slug/favorite`. The handler
/// will read the `slug` path parameter value, unfavorite the article using the currently authenticated
/// user and return the data for the matching article if it exists, otherwise it will return a 404
/// response.
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
async fn unfavorite_article(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path(slug): Path<String>,
) -> Result<Response, Error> {
    match fetch_article_row_by_slug(&ctx.db, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(row) => {
            sqlx::query(DELETE_USER_ARTICLE_FAV_QUERY)
                .bind(row.id)
                .bind(auth_ctx.user_id)
                .execute(&ctx.db)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            let article = fetch_article(&ctx.db, &slug, Some(auth_ctx.user_id))
                .await?
                .expect("article exists");

            Ok(Json(ArticleBody { article }).into_response())
        }
    }
}

/// Retrieves the [`Article`] from the database by slug, if it exists, with the specified user
/// context to determine favorite and profile follow status.
async fn fetch_article(
    db: &PgPool,
    slug: &str,
    user_ctx: Option<Uuid>,
) -> Result<Option<Article>, Error> {
    match fetch_article_view_by_slug(db, slug, user_ctx).await? {
        None => Ok(None),
        Some(view) => {
            let tags = sqlx::query_as(GET_TAGS_FOR_ARTICLE_QUERY)
                .bind(view.id)
                .fetch_all(db)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?
                .into_iter()
                .map(|t: Tag| t.name)
                .collect();

            let author = profile::fetch_profile_by_id(db, &view.user_id, user_ctx)
                .await?
                .expect("authenticated user exists");

            let article = Article::with_view_tags_and_profile(view, tags, author);

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

    let mut tx = db.begin().await?;

    let row: ArticleRow = sqlx::query_as(CREATE_ARTICLE_QUERY)
        .bind(user_id)
        .bind(slug)
        .bind(&article.title)
        .bind(&article.description)
        .bind(&article.body)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    if let Some(tags) = &article.tags {
        // TODO: could probably be more efficient
        for name in tags {
            let tag: Tag = sqlx::query_as(CREATE_TAG_QUERY)
                .bind(name)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;

            let _ = sqlx::query(CREATE_ARTICLE_TAG_QUERY)
                .bind(row.id)
                .bind(tag.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    tracing::error!("error returned from the database: {}", e);
                    Error::from(e)
                })?;
        }
    }

    tx.commit().await?;

    Ok(row)
}

/// Retrives an [`ArticleRow`] from the articles table in the database identified by the given slug.
async fn fetch_article_row_by_slug(db: &PgPool, slug: &str) -> Result<Option<ArticleRow>, Error> {
    sqlx::query_as(GET_ARTICLE_BY_SLUG_QUERY)
        .bind(slug)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Retrieves an [`ArticleView`] for an article identified by the given slug using the identifier of
/// the authenticated user, if available, as the user context to determine if the article is favorited
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

/// Retrives a [`Vec`] of [`ArticleView`]s that make up a page of articles in the feed of the
/// specified user.
async fn fetch_article_views_for_user_feed(
    db: &PgPool,
    user_id: &Uuid,
    page: &Pagination,
) -> Result<Vec<ArticleView>, Error> {
    sqlx::query_as(GET_USER_FEED_PAGE_QUERY)
        .bind(user_id)
        .bind(page.limit)
        .bind(page.offset)
        .fetch_all(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}
