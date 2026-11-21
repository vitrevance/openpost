-- openpost schema
-- accounts.id is a server-generated UUIDv4 that doubles as the access token.
CREATE TABLE accounts (
    id          UUID PRIMARY KEY,
    short_name  TEXT NOT NULL CHECK (char_length(short_name) BETWEEN 1 AND 64),
    author_name TEXT,
    author_url  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE pages (
    id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    path           TEXT NOT NULL UNIQUE,
    account_id     UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    title          TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 256),
    content        TEXT NOT NULL,
    content_format TEXT NOT NULL DEFAULT 'markdown' CHECK (content_format IN ('markdown', 'html')),
    author_name    TEXT,
    author_url     TEXT,
    html_cache     TEXT,
    views          BIGINT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pages_account_created ON pages (account_id, created_at DESC);
