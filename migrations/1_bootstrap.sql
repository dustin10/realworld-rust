-- enable uuid funcs
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- create a function that will set the updated column to now. if desired one could manage
-- the updated column in the application itself.
CREATE OR REPLACE FUNCTION set_updated()
  RETURNS TRIGGER 
  LANGUAGE PLPGSQL
  AS
$$
BEGIN
  NEW.updated = NOW();
	RETURN NEW;
END
$$;

-- create the table to store user data
CREATE TABLE IF NOT EXISTS users (
  id UUID PRIMARY KEY DEFAULT UUID_GENERATE_V4(),
  name TEXT UNIQUE NOT NULL,
  email TEXT UNIQUE NOT NULL,
  bio TEXT NOT NULL DEFAULT '',
  image TEXT,
  password TEXT NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated TIMESTAMPTZ
);

-- create trigger on user table that sets updated column when a row is changed
CREATE TRIGGER users_set_updated
    BEFORE UPDATE
    ON users
    FOR EACH ROW
    WHEN (OLD IS DISTINCT FROM NEW)
    EXECUTE FUNCTION set_updated();

-- create the user_follow table to store mapping between users
CREATE TABLE IF NOT EXISTS user_follows (
  user_id UUID NOT NULL,
  follower_id UUID NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY(user_id, follower_id),
  CONSTRAINT fk_uid FOREIGN KEY(user_id) REFERENCES users(id),
  CONSTRAINT fk_fid FOREIGN KEY(follower_id) REFERENCES users(id)
);

-- create the articles table to store articles created by users
CREATE TABLE IF NOT EXISTS articles (
  id UUID PRIMARY KEY DEFAULT UUID_GENERATE_V4(),
  user_id UUID NOT NULL,
  slug TEXT UNIQUE NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  body TEXT NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated TIMESTAMPTZ,
  CONSTRAINT fk_uid FOREIGN KEY(user_id) REFERENCES users(id)
);

-- create trigger on articles table that sets updated column when a row is changed
CREATE TRIGGER articles_set_updated
    BEFORE UPDATE
    ON articles
    FOR EACH ROW
    WHEN (OLD IS DISTINCT FROM NEW)
    EXECUTE FUNCTION set_updated();

-- create the tags table to store tags associated with articles
CREATE TABLE IF NOT EXISTS tags (
  id UUID PRIMARY KEY DEFAULT UUID_GENERATE_V4(),
  name TEXT UNIQUE NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- create the article_tags table to store mapping between articles and tags
CREATE TABLE IF NOT EXISTS article_tags (
  article_id UUID NOT NULL,
  tag_id UUID NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY(article_id, tag_id),
  CONSTRAINT fk_aid FOREIGN KEY(article_id) REFERENCES articles(id),
  CONSTRAINT fk_tid FOREIGN KEY(tag_id) REFERENCES tags(id)
);

-- create the article_favs table to store mapping between users and their
-- favorited articles
CREATE TABLE IF NOT EXISTS article_favs (
  article_id UUID NOT NULL,
  user_id UUID NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY(article_id, user_id),
  CONSTRAINT fk_aid FOREIGN KEY(article_id) REFERENCES articles(id),
  CONSTRAINT fk_uid FOREIGN KEY(user_id) REFERENCES users(id)
);

-- create the article_comments table that stores comments made on articles
-- by users
CREATE TABLE IF NOT EXISTS article_comments (
  id UUID PRIMARY KEY DEFAULT UUID_GENERATE_V4(),
  user_id UUID NOT NULL,
  article_id UUID NOT NULL,
  body TEXT NOT NULL,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT fk_uid FOREIGN KEY(user_id) REFERENCES users(id),
  CONSTRAINT fk_aid FOREIGN KEY(article_id) REFERENCES articles(id)
);

-- create the outbox table that allows for transactional event publishing
CREATE TABLE IF NOT EXISTS outbox (
  id UUID PRIMARY KEY DEFAULT UUID_GENERATE_V4(),
  topic TEXT NOT NULL,
  partition_key TEXT,
  headers JSONB,
  payload JSONB,
  created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
)
