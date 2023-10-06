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
CREATE TABLE IF NOT EXISTS "user" (
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
CREATE TRIGGER user_set_updated
    BEFORE UPDATE
    ON "user"
    FOR EACH ROW
    WHEN (OLD IS DISTINCT FROM NEW)
    EXECUTE FUNCTION set_updated();
