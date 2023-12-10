use crate::db::tag::Tag;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch a single page of the article feed for a user.
const GET_USER_FEED_PAGE_QUERY: &str = r#"
    SELECT
        a.*,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id AND af.user_id = $1)::int::bool AS favorited,
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id) as favorites_count,
        (ARRAY_TO_STRING(ARRAY(SELECT t.name FROM tags AS t INNER JOIN article_tags AS at ON t.id = at.tag_id WHERE at.article_id = a.id), ',')) AS tags,
        u.name AS author_name,
        u.bio AS author_bio,
        u.image AS author_image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS author_followed
    FROM
        articles AS a INNER JOIN users AS u ON a.user_id = u.id INNER JOIN user_follows AS uf ON a.user_id = uf.user_id
    WHERE
        uf.follower_id = $1
    ORDER BY
        a.created DESC
    LIMIT
        $2
    OFFSET
        $3"#;

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
        (SELECT COUNT(af.*) FROM article_favs AS af WHERE af.article_id = a.id) as favorites_count,
        (ARRAY_TO_STRING(ARRAY(SELECT t.name FROM tags AS t INNER JOIN article_tags AS at ON t.id = at.tag_id WHERE at.article_id = a.id), ',')) AS tags,
        u.name AS author_name,
        u.bio AS author_bio,
        u.image AS author_image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS author_followed
    FROM
        articles AS a INNER JOIN users AS u ON a.user_id = u.id LEFT JOIN user_follows AS uf ON a.user_id = uf.user_id
     WHERE
        a.slug = $2"#;

/// SQL query used to delete entries from the user favorites join table for an article.
const DELETE_ARTICLE_FAVS_QUERY: &str = "DELETE FROM article_favs WHERE article_id = $1";

/// SQL query used to delete the links from a tag to an article.
const DELETE_ARTICLE_TAGS_QUERY: &str = "DELETE FROM article_tags WHERE article_id = $1";

/// SQL query used to delete an article.
const DELETE_ARTICLE_QUERY: &str = "DELETE FROM articles WHERE id = $1";

/// SQL query used to create a new comment for an article.
const CREATE_ARTICLE_COMMENT_QUERY: &str = r#"
    WITH inserted_comment AS (
        INSERT INTO article_comments (user_id, article_id, body) VALUES ($1, $2, $3) RETURNING *
    )
    SELECT
        ic.id,
        ic.user_id,
        ic.article_id,
        ic.body,
        ic.created,
        u.name AS author_name,
        u.bio AS author_bio,
        u.image AS author_image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS author_followed
    FROM
        inserted_comment AS ic INNER JOIN users AS u ON ic.user_id = u.id"#;

/// SQL query used to delete a comment from an article.
const DELETE_ARTICLE_COMMENT_QUERY: &str =
    "DELETE FROM article_comments WHERE id = $1 AND user_id = $2";

/// SQL query used to fetch the comments for a single article by slug.
const GET_ARTICLE_COMMENTS_BY_SLUG_QUERY: &str = r#"
    SELECT
        ac.*,
        u.name AS author_name,
        u.bio AS author_bio,
        u.image AS author_image,
        (SELECT COUNT(*) FROM user_follows AS uf WHERE uf.user_id = u.id AND uf.follower_id = $1)::int::bool AS author_followed
    FROM
        article_comments AS ac INNER JOIN articles AS a ON ac.article_id = a.id INNER JOIN users AS u ON ac.user_id = u.id
    WHERE
        a.slug = $2
    ORDER BY
        ac.created ASC"#;

/// SQL query used to create an entry in the table that captures favorited articles for a user.
const CREATE_USER_ARTICLE_FAV_QUERY: &str = r#"
    WITH target_article AS (
        SELECT slug FROM articles WHERE id = $1
    ), inserted_fav AS (
        INSERT INTO article_favs (article_id, user_id) VALUES($1, $2) ON CONFLICT DO NOTHING
    )
    SELECT slug FROM target_article"#;

/// SQL query used to delete the entry in the table that captures favorited articles for a user.
const DELETE_USER_ARTICLE_FAV_QUERY: &str = r#"
    WITH target_article AS (
        SELECT slug FROM articles WHERE id = $1
    ), deleted_fav AS (
        DELETE FROM article_favs WHERE article_id = $1 AND user_id = $2
    )
    SELECT slug FROM target_article"#;

/// The [`Article`] struct is used to let the `sqlx` library easily map a row from the `articles`
/// table in the database to a struct value. It is a one-to-one mapping from the database table.
#[derive(Debug, FromRow)]
pub struct Article {
    /// Id of the article.
    pub id: Uuid,
    /// Id of the user who authored the article.
    #[allow(dead_code)]
    pub user_id: Uuid,
    /// Slugified title of the article.
    pub slug: String,
    /// Title of the article.
    #[allow(dead_code)]
    pub title: String,
    /// Description of the article.
    #[allow(dead_code)]
    pub description: String,
    /// Body of the article.
    #[allow(dead_code)]
    pub body: String,
    /// Time the article was created.
    #[allow(dead_code)]
    pub created: DateTime<Utc>,
    /// Time the article was last modified.
    #[allow(dead_code)]
    pub updated: Option<DateTime<Utc>>,
}

/// The [`ArticleView`] struct is used to let the `sqlx` library easily map a view of the `articles`
/// table and supporting data in the database to a struct value. This is not a one-to-one mapping
/// from the row to the struct but rather there is also some computed data returned. Hence, the
/// name view. Some people may also refer to this as a projection.
#[derive(Debug, FromRow)]
pub struct ArticleView {
    /// Slugified title of the article.
    pub slug: String,
    /// Title of the article.
    pub title: String,
    /// Description of the article.
    pub description: String,
    /// Body of the article.
    pub body: String,
    /// CSV of tags associated with the article.
    pub tags: Option<String>,
    /// Time the article was created.
    pub created: DateTime<Utc>,
    /// Time the article was last modified.
    pub updated: Option<DateTime<Utc>>,
    /// Flag indicating whether the logged in user, if available, has favorited the article.
    pub favorited: bool,
    /// Count of the total number of users who have favorited the article.
    pub favorites_count: i64,
    /// Username of the author.
    pub author_name: String,
    /// Bio for the the author.
    pub author_bio: String,
    /// URL to the image of the author.
    pub author_image: Option<String>,
    /// Flag indicating whether or not the author is being followed by the currently authenticated
    /// user. If no user is curently logged in, then the value will be set to `false`.
    pub author_followed: bool,
}

/// The [`CreateArticle`] struct contains the data required to create an article in the database.
#[derive(Debug)]
pub struct CreateArticle<'a> {
    /// Title of the article.
    pub title: &'a String,
    /// Description of the article.
    pub description: &'a String,
    /// Body of the article.
    pub body: &'a String,
    /// List of tags associated with the article.
    pub tags: Option<&'a Vec<String>>,
}

/// The [`Comment`] struct is used to let the `sqlx` library easily map a row from the `comments`
/// table in the database to a struct value. It is a one-to-one mapping from the database table.
#[derive(Debug, FromRow)]
pub struct Comment {
    /// Id of the comment.
    pub id: Uuid,
    /// Id of the user who authored the comment.
    pub user_id: Uuid,
    /// Id of the article the comment was made on.
    #[allow(dead_code)]
    pub article_id: Uuid,
    /// Body text of the comment.
    pub body: String,
    /// Time at which the comment was made.
    pub created: DateTime<Utc>,
}

/// The [`CommentView`] struct is used to let the `sqlx` library easily map a view of the `comments`
/// table and supporting data in the database to a struct value. This is not a one-to-one mapping
/// from the row to the struct but rather there is also some computed data returned. Hence, the
/// name view. Some people may also refer to this as a projection.
#[derive(Debug, FromRow)]
pub struct CommentView {
    /// Id of the comment.
    pub id: Uuid,
    /// Body text of the comment.
    pub body: String,
    /// Time at which the comment was made.
    pub created: DateTime<Utc>,
    /// Username of the author.
    pub author_name: String,
    /// Bio for the the author.
    pub author_bio: String,
    /// URL to the image of the author.
    pub author_image: Option<String>,
    /// Flag indicating whether or not the author is being followed by the currently authenticated
    /// user. If no user is curently logged in, then the value will be set to `false`.
    pub author_followed: bool,
}

/// The [`CreateComment`] struct contains the data required to create a comment on an article in
/// the database.
#[derive(Debug)]
pub struct CreateComment<'a> {
    /// Id of the user who authored the comment.
    pub user_id: &'a Uuid,
    /// Text of the comment.
    pub body: &'a String,
}

/// Transactionally creates a new [`Article`] row in the database using the details contained in
/// the given a [`CreateArticle`].
pub async fn create_article(
    db: &PgPool,
    user_id: &Uuid,
    article: &CreateArticle<'_>,
) -> Result<ArticleView, sqlx::Error> {
    // TODO: this is naive and will fail if an article with the same title exists. we could append
    // a number in that case but that could degenerate to a lot fo queries if colliding titles is a
    // common occurent. we could probably append the date formatted in a url friendly way to mostly
    // avoid these collisions.
    let slug = slug::slugify(article.title);

    let mut tx = db.begin().await?;

    let row: Article = sqlx::query_as(CREATE_ARTICLE_QUERY)
        .bind(user_id)
        .bind(slug)
        .bind(article.title)
        .bind(article.description)
        .bind(article.body)
        .fetch_one(&mut *tx)
        .await?;

    if let Some(tags) = article.tags {
        // TODO: could probably be more efficient
        for name in tags {
            let tag: Tag = sqlx::query_as(CREATE_TAG_QUERY)
                .bind(name)
                .fetch_one(&mut *tx)
                .await?;

            let _ = sqlx::query(CREATE_ARTICLE_TAG_QUERY)
                .bind(row.id)
                .bind(tag.id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;

    query_article_view_by_slug(db, &row.slug, None)
        .await
        .map(|av| av.expect("article should exist"))
}

/// Retrieves an [`Article`] identified by the given slug, if it exists.
pub async fn query_article_by_slug(
    db: &PgPool,
    slug: &str,
) -> Result<Option<Article>, sqlx::Error> {
    sqlx::query_as(GET_ARTICLE_BY_SLUG_QUERY)
        .bind(slug)
        .fetch_optional(db)
        .await
}

/// Retrieves an [`ArticleView`] identified by the given slug, if it exsts, using the
/// identifier of the authenticated user, if available, as the user context to determine
/// if the article is favorited or not.
pub async fn query_article_view_by_slug(
    db: &PgPool,
    slug: &str,
    user_ctx: Option<Uuid>,
) -> Result<Option<ArticleView>, sqlx::Error> {
    let user_context = user_ctx.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_ARTICLE_VIEW_BY_SLUG_QUERY)
        .bind(user_context)
        .bind(slug)
        .fetch_optional(db)
        .await
}

/// Retrives a [`Vec`] of [`ArticleView`]s that make up a page of articles in the feed of the
/// specified user.
pub async fn query_user_feed(
    db: &PgPool,
    user_ctx: &Uuid,
    limit: i32,
    offset: i32,
) -> Result<Vec<ArticleView>, sqlx::Error> {
    sqlx::query_as(GET_USER_FEED_PAGE_QUERY)
        .bind(user_ctx)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await
}

/// Transactionally deletes an [`Article`] and any existing relational data given the identifier.
pub async fn delete_article_by_id(db: &PgPool, article_id: &Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;

    // delete any favorites
    let _ = sqlx::query(DELETE_ARTICLE_FAVS_QUERY)
        .bind(article_id)
        .execute(&mut *tx)
        .await?;

    // delete any tags associations
    let _ = sqlx::query(DELETE_ARTICLE_TAGS_QUERY)
        .bind(article_id)
        .execute(&mut *tx)
        .await?;

    // finally delete the article
    let _ = sqlx::query(DELETE_ARTICLE_QUERY)
        .bind(article_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await
}

/// Inserts an entry into the article comments table. Returns the [`CommentView`] that represnts
/// the newly created comment.
pub async fn add_article_comment(
    db: &PgPool,
    article_id: &Uuid,
    comment: &CreateComment<'_>,
) -> Result<CommentView, sqlx::Error> {
    sqlx::query_as(CREATE_ARTICLE_COMMENT_QUERY)
        .bind(comment.user_id)
        .bind(article_id)
        .bind(comment.body)
        .fetch_one(db)
        .await
}

/// Deletes an the entry from the article comments table that matches the comment and user
/// identifiers.
pub async fn remove_article_comment(
    db: &PgPool,
    comment_id: &Uuid,
    user_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(DELETE_ARTICLE_COMMENT_QUERY)
        .bind(comment_id)
        .bind(user_id)
        .execute(db)
        .await
        .map(|_| ())
}

/// Retrives a [`Vec`] that contains all of the [`CommentView`]s that are associated to an article.
/// using the identifier of the authenticated user, if available, as the user context to determine
/// if the author followed status.
pub async fn query_article_comments_by_slug(
    db: &PgPool,
    slug: &str,
    user_ctx: Option<Uuid>,
) -> Result<Vec<CommentView>, sqlx::Error> {
    let user_context = user_ctx.unwrap_or_else(Uuid::nil);

    sqlx::query_as(GET_ARTICLE_COMMENTS_BY_SLUG_QUERY)
        .bind(user_context)
        .bind(slug)
        .fetch_all(db)
        .await
}

/// The [`SlugW`] struct is a smaller wrapper around a String that makes it easy to deserialize a
/// value returned from the database query when favoriting or unfavoriting an article.
#[derive(Debug, FromRow)]
struct SlugW {
    slug: String,
}

/// Inserts an entry into the table that tracks favorited articles for a user and returns the
/// [`ArticleView`] of the newly favorited article.
pub async fn add_article_favorite(
    db: &PgPool,
    article_id: &Uuid,
    user_id: &Uuid,
) -> Result<ArticleView, sqlx::Error> {
    let slug: SlugW = sqlx::query_as(CREATE_USER_ARTICLE_FAV_QUERY)
        .bind(article_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

    query_article_view_by_slug(db, &slug.slug, Some(*user_id))
        .await
        .map(|av| av.expect("article should exist"))
}

/// Deletes an entry from the table that tracks favorited articles for a user and returns the
/// [`ArticleView`] of the newly unfavorited article.
pub async fn remove_article_favorite(
    db: &PgPool,
    article_id: &Uuid,
    user_id: &Uuid,
) -> Result<ArticleView, sqlx::Error> {
    let slug: SlugW = sqlx::query_as(DELETE_USER_ARTICLE_FAV_QUERY)
        .bind(article_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

    query_article_view_by_slug(db, &slug.slug, Some(*user_id))
        .await
        .map(|av| av.expect("article should exist"))
}
