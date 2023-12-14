use std::collections::HashMap;

use crate::{
    db,
    db::{outbox::CreateOutboxEntry, user::Profile},
    http::{auth::AuthContext, AppContext, Error, Pagination},
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
use uuid::Uuid;

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
        .route("/api/articles", get(list_articles).post(create_article))
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

/// The [`Article`] struct contains data that repesents an article as returned from the API. It
/// contains the relevant article data, tag data and properties relevant to the currently
/// authenticted user if one exists.
#[derive(Debug, Serialize)]
struct Article {
    /// Id of the article.
    #[serde(skip_serializing)]
    id: Uuid,
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
    /// Creates a new [`Article`] populated from the given [`crate::db::article::ArticleView`].
    fn with_db_view(view: db::article::ArticleView) -> Self {
        // TODO: Consider storing articles tags in an array directly on the article row in the database.
        // Right now we send back a CSV of tags with the query result and then they are transformed into a
        // Vec<String> before the response is returned to the client. Having that tags in their own table
        // allows for an easy implementation of the list tags API so that could be kept along side the
        // text array property on the article.
        let tags = match view.tags {
            Some(csv) if !csv.is_empty() => Some(csv.split(',').map(ToOwned::to_owned).collect()),
            _ => None,
        };

        Self {
            id: view.id,
            slug: view.slug,
            title: view.title,
            description: view.description,
            body: view.body,
            created: view.created,
            updated: view.updated,
            favorited: view.favorited,
            favorites_count: view.favorites_count,
            tags,
            author: Profile {
                id: view.author_id,
                name: view.author_name,
                bio: view.author_bio,
                image: view.author_image,
                follows: view.author_followed,
            },
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
    /// Total count of the articles matching any filters.
    #[serde(rename = "articlesCount")]
    articles_count: i64,
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
    /// Creates a new [`Comment`] from the given [`crate::db::article::CommentView`].
    fn with_db_view(view: db::article::CommentView) -> Self {
        Self {
            id: view.id,
            body: view.body,
            created: view.created,
            author: Profile {
                id: view.author_id,
                name: view.author_name,
                bio: view.author_bio,
                image: view.author_image,
                follows: view.author_followed,
            },
        }
    }
}

/// The [`ListFilters`] struct encapsulates all of the filters available to the list articles
/// API. The axum framework can deserialize the query string parameters into an instance of
/// the struct auto-magically for us.
#[derive(Debug, Deserialize)]
struct ListFilters {
    /// Name of the tag that an article must have.
    tag: Option<String>,
    /// Name of the author of the article.
    author: Option<String>,
    /// Name of the user who favorited the article.
    favorited: Option<String>,
    // TODO: this would be preferable but appears to be a limitation in serde when trying to do
    // this. A workaround is given here https://docs.rs/serde_qs/0.12.0/serde_qs/index.html#flatten-workaround
    // so perhaps look into it at some point. Until then just duplicate the fields.
    //#[serde(flatten)]
    //page: Pagination,
    /// Maximum number of results to return for a single request.
    #[serde(default = "crate::http::default_limit")]
    limit: i32,
    /// Starting offset into the entire set of results.
    #[serde(default)]
    offset: i32,
}

#[derive(Debug, Serialize)]
struct Author {
    id: Uuid,
    name: String,
}

/// The [`ArticleEvent`] struct contains event data related to an article that is published to Kafka
/// when the article is created, updated or deleted.
#[derive(Debug, Serialize)]
struct ArticleEvent {
    /// Id of the article.
    id: Uuid,
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
    /// Any tags that have been set on the article.
    #[serde(rename = "tagList")]
    tags: Option<Vec<String>>,
    /// Author of the article.
    author: Author,
}

impl ArticleEvent {
    /// Creates a new [`ArticleEvent`] from the data in the given [`Article`].
    fn with_article(article: &Article) -> Self {
        Self {
            id: article.id,
            slug: article.slug.clone(),
            title: article.title.clone(),
            description: article.description.clone(),
            body: article.body.clone(),
            created: article.created,
            updated: article.updated,
            tags: article.tags.clone(),
            author: Author {
                id: article.author.id,
                name: article.author.name.clone(),
            },
        }
    }
}

/// The [`ArticleEvent`] struct contains event data related to an article comment that is published
/// to Kafka when the article is created or deleted.
#[derive(Debug, Serialize)]
struct CommentEvent {
    /// Id of the comment.
    id: Uuid,
    /// Text of the comment.
    body: String,
    /// Time the comment was created.
    created: DateTime<Utc>,
    /// Author of the comment.
    author: Author,
}

impl CommentEvent {
    /// Creates a new [`CommentEvent`] from the given [`Comment`].
    fn with_comment(comment: &Comment) -> Self {
        Self {
            id: comment.id,
            body: comment.body.clone(),
            created: comment.created,
            author: Author {
                id: comment.author.id,
                name: comment.author.name.clone(),
            },
        }
    }
}

/// Handles the list articles endpoint at `GET /api/articles` which returns articles ordered by
/// created date in descending order.
///
/// # Query Parameters
///
/// The following query parameters are supported which allow the client to filter the articles
/// returned by the API.
///
/// * `tag` - name of the tag associated with the article
/// * `author` - name of the user who authored the article
/// * `favorited` - name of the user who favorited the article
/// * `limit` - count of the articles that should be returned in the response
/// * `offset` - offset into the total set of results to start the current result set
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
async fn list_articles(
    ctx: State<AppContext>,
    auth_ctx: Option<AuthContext>,
    filters: Query<ListFilters>,
) -> Result<Json<ArticlesBody>, Error> {
    let user_ctx = auth_ctx.map(|ac| ac.user_id);

    let mut tx = ctx.db.begin().await?;

    let articles = db::article::query_articles(
        &mut tx,
        user_ctx,
        filters.tag.as_ref(),
        filters.author.as_ref(),
        filters.favorited.as_ref(),
        filters.limit,
        filters.offset,
    )
    .await?
    .into_iter()
    .map(Article::with_db_view)
    .collect();

    let articles_count = db::article::count_articles(
        &mut tx,
        filters.tag.as_ref(),
        filters.author.as_ref(),
        filters.favorited.as_ref(),
    )
    .await?;

    Ok(Json(ArticlesBody {
        articles,
        articles_count,
    }))
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
    let mut tx = ctx.db.begin().await?;

    let articles =
        db::article::query_user_feed(&mut tx, &auth_ctx.user_id, page.0.limit, page.0.offset)
            .await?
            .into_iter()
            .map(Article::with_db_view)
            .collect();

    let articles_count = db::article::count_user_feed(&mut tx, &auth_ctx.user_id).await?;

    tx.commit().await?;

    Ok(Json(ArticlesBody {
        articles,
        articles_count,
    }))
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
    let create_article = db::article::CreateArticle {
        title: &request.article.title,
        description: &request.article.description,
        body: &request.article.body,
        tags: request.article.tags.as_ref(),
    };

    let mut tx = ctx.db.begin().await?;

    let article = db::article::create_article(&mut tx, &auth_ctx.user_id, &create_article)
        .await
        .map(Article::with_db_view)?;

    let article_event = ArticleEvent::with_article(&article);

    let mut headers = HashMap::with_capacity(1);
    headers.insert(String::from("type"), String::from("ARTICLE_CREATED"));

    let create_outbox_entry = db::outbox::CreateOutboxEntry {
        topic: String::from("article"),
        partition_key: Some(article_event.id.to_string()),
        headers: Some(headers),
        payload: Some(article_event),
    };

    let _ = db::outbox::create_outbox_entry(&mut tx, create_outbox_entry).await?;

    tx.commit().await?;

    Ok(Json(ArticleBody { article }).into_response())
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

    let mut tx = ctx.db.begin().await?;

    let response = match db::article::query_article_view_by_slug(&mut tx, &slug, user_ctx).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(db_view) => {
            let article = Article::with_db_view(db_view);

            Ok(Json(ArticleBody { article }).into_response())
        }
    };

    tx.commit().await?;

    response
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
    let mut tx = ctx.db.begin().await?;

    let response = match db::article::query_article_by_slug(&mut tx, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            if auth_ctx.user_id != article.user_id {
                return Ok(StatusCode::FORBIDDEN.into_response());
            }

            db::article::delete_article_by_id(&mut tx, &article.id).await?;

            let mut headers = HashMap::with_capacity(1);
            headers.insert(String::from("type"), String::from("ARTICLE_DELETED"));

            let create_outbox_entry: CreateOutboxEntry<()> = db::outbox::CreateOutboxEntry {
                topic: String::from("article"),
                partition_key: Some(article.id.to_string()),
                headers: Some(headers),
                payload: None,
            };

            let _ = db::outbox::create_outbox_entry(&mut tx, create_outbox_entry).await?;

            Ok(StatusCode::NO_CONTENT.into_response())
        }
    };

    tx.commit().await?;

    response
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
///   "comment":
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
    let mut tx = ctx.db.begin().await?;

    let response = match db::article::query_article_by_slug(&mut tx, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            let data = db::article::CreateComment {
                user_id: &auth_ctx.user_id,
                body: &request.comment.body,
            };

            let comment = db::article::add_article_comment(&mut tx, &article.id, &data)
                .await
                .map(Comment::with_db_view)?;

            let mut headers = HashMap::with_capacity(1);
            headers.insert(String::from("type"), String::from("COMMENT_CREATED"));

            let comment_event = CommentEvent::with_comment(&comment);

            let create_outbox_entry = db::outbox::CreateOutboxEntry {
                topic: String::from("article"),
                partition_key: Some(article.id.to_string()),
                headers: Some(headers),
                payload: Some(comment_event),
            };

            let _ = db::outbox::create_outbox_entry(&mut tx, create_outbox_entry).await?;

            Ok(Json(CommentBody { comment }).into_response())
        }
    };

    tx.commit().await?;

    response
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
    let user_ctx = auth_ctx.map(|ac| ac.user_id);

    let mut tx = ctx.db.begin().await?;

    let comments = db::article::query_article_comments_by_slug(&mut tx, &slug, user_ctx)
        .await?
        .into_iter()
        .map(Comment::with_db_view)
        .collect();

    tx.commit().await?;

    Ok(Json(CommentsBody { comments }))
}

/// Handles the delete article comment API endpoint at `DELETE /api/articles/:slug/comments/:id`.
async fn delete_comment(
    ctx: State<AppContext>,
    auth_ctx: AuthContext,
    Path((slug, id)): Path<(String, Uuid)>,
) -> Result<Response, Error> {
    let mut tx = ctx.db.begin().await?;

    let response = match db::article::query_article_by_slug(&mut tx, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            // TODO: we could do better here by checking affected rows affected and returning 404 if zero
            db::article::remove_article_comment(&mut tx, &id, &auth_ctx.user_id).await?;

            let mut headers = HashMap::with_capacity(1);
            headers.insert(String::from("type"), String::from("COMMENT_DELETED"));

            let create_outbox_entry: CreateOutboxEntry<()> = db::outbox::CreateOutboxEntry {
                topic: String::from("article"),
                partition_key: Some(article.id.to_string()),
                headers: Some(headers),
                payload: None,
            };

            let _ = db::outbox::create_outbox_entry(&mut tx, create_outbox_entry).await?;

            Ok(StatusCode::NO_CONTENT.into_response())
        }
    };

    tx.commit().await?;

    response
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
    let mut tx = ctx.db.begin().await?;

    // TODO: handle case where favorite entry already exists
    let response = match db::article::query_article_by_slug(&mut tx, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            let article =
                db::article::add_article_favorite(&mut tx, &article.id, &auth_ctx.user_id)
                    .await
                    .map(Article::with_db_view)?;

            Ok(Json(ArticleBody { article }).into_response())
        }
    };

    tx.commit().await?;

    response
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
    let mut tx = ctx.db.begin().await?;

    let response = match db::article::query_article_by_slug(&mut tx, &slug).await? {
        None => Ok(StatusCode::NOT_FOUND.into_response()),
        Some(article) => {
            let article =
                db::article::remove_article_favorite(&mut tx, &article.id, &auth_ctx.user_id)
                    .await
                    .map(Article::with_db_view)?;

            Ok(Json(ArticleBody { article }).into_response())
        }
    };

    tx.commit().await?;

    response
}
