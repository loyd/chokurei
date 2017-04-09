CREATE TABLE entry (
    id          BIGSERIAL   PRIMARY KEY,
    key         TEXT        NOT NULL UNIQUE,
    feed_id     INTEGER     NOT NULL REFERENCES feed ON DELETE CASCADE,
    url         TEXT        CHECK (ltrim(url) <> ''),
    title       TEXT        CHECK (ltrim(title) <> ''),
    author      TEXT        CHECK (ltrim(author) <> ''),
    description TEXT        CHECK (ltrim(description) <> ''),
    content     TEXT        CHECK (ltrim(content) <> ''),
    published   TIMESTAMP
);

CREATE INDEX entry_feed_id_idx on entry (feed_id);
