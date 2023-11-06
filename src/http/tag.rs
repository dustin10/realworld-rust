use crate::http::Error;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// SQL query used to fetch the tags for an article from the database.
const GET_TAGS_FOR_ARTICLE_QUERY: &str = r#"
    SELECT
        t.*
    FROM
        tags AS t INNER JOIN article_tags AS at ON t.id = at.tag_id
    WHERE
        at.article_id = $1"#;

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

/// SQL query used to delete the links from a tag to an article.
const DELETE_ARTICLE_TAGS_QUERY: &str = "DELETE FROM article_tags WHERE article_id = $1";

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

/// Retrieves all of the [`Tag`]s that are associated to the article with the specifid id.
pub(crate) async fn fetch_tags_for_article(
    db: &PgPool,
    article_id: &Uuid,
) -> Result<Vec<Tag>, Error> {
    sqlx::query_as(GET_TAGS_FOR_ARTICLE_QUERY)
        .bind(article_id)
        .fetch_all(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Inserts a row into the tags table.
pub(crate) async fn insert_tag(db: &PgPool, name: &str) -> Result<Tag, Error> {
    sqlx::query_as(CREATE_TAG_QUERY)
        .bind(name)
        .fetch_one(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })
}

/// Inserts a row into the table that associates tags to an article.
pub(crate) async fn insert_article_tag(
    db: &PgPool,
    article_id: &Uuid,
    tag_id: &Uuid,
) -> Result<(), Error> {
    let _ = sqlx::query(CREATE_ARTICLE_TAG_QUERY)
        .bind(article_id)
        .bind(tag_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    Ok(())
}

pub(crate) async fn delete_article_tags(db: &PgPool, article_id: &Uuid) -> Result<(), Error> {
    let _ = sqlx::query(DELETE_ARTICLE_TAGS_QUERY)
        .bind(article_id)
        .execute(db)
        .await
        .map_err(|e| {
            tracing::error!("error returned from the database: {}", e);
            Error::from(e)
        })?;

    Ok(())
}
