CREATE TABLE feed (
    id          SERIAL  PRIMARY KEY,
    key         TEXT    NOT NULL UNIQUE,
    url         TEXT    NOT NULL,
    title       TEXT    CHECK (ltrim(title) <> ''),
    website     TEXT    CHECK (ltrim(website) <> ''),
    description TEXT    CHECK (ltrim(description) <> ''),
    language    CHAR(2) CHECK (language ~ '^[a-z][a-z]$'),  -- ISO 639-1
    copyright   TEXT    CHECK (ltrim(copyright) <> ''),
    interval    INTEGER CHECK (interval > 0)
);
