CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Core entities
CREATE TABLE IF NOT EXISTS notebooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    last_viewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    asset JSONB,
    title VARCHAR,
    topics JSONB NOT NULL DEFAULT '[]'::jsonb,
    full_text TEXT,
    last_viewed_at TIMESTAMPTZ,
    command_id UUID,
    -- ponytail: status/processing_info mirror async flow fields used by frontend; refine when job pipeline lands.
    status VARCHAR,
    processing_info JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS source_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding vector,
    chunk_order INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS source_insights (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    insight_type VARCHAR NOT NULL,
    content TEXT NOT NULL,
    embedding vector,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR,
    note_type VARCHAR DEFAULT 'human',
    content TEXT,
    embedding vector,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Relationships
CREATE TABLE IF NOT EXISTS source_notebooks (
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    notebook_id UUID NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, notebook_id)
);

CREATE TABLE IF NOT EXISTS note_notebooks (
    note_id UUID NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    notebook_id UUID NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, notebook_id)
);

-- Chat / commands / settings placeholders for upcoming sections
CREATE TABLE IF NOT EXISTS chat_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR,
    model_override VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    role VARCHAR NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS chat_session_notebooks (
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    notebook_id UUID NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, notebook_id)
);

CREATE TABLE IF NOT EXISTS chat_session_sources (
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, source_id)
);

CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    app_name VARCHAR NOT NULL,
    command_name VARCHAR NOT NULL,
    input JSONB NOT NULL,
    status VARCHAR NOT NULL DEFAULT 'pending',
    result JSONB,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 5,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS settings (
    key VARCHAR PRIMARY KEY,
    value JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Full-text helper index (vector indexes added when embeddings pipeline lands)
CREATE INDEX IF NOT EXISTS idx_sources_full_text_tsv
    ON sources USING GIN (to_tsvector('english', coalesce(full_text, '') || ' ' || coalesce(title, '')));
CREATE INDEX IF NOT EXISTS idx_notes_content_tsv
    ON notes USING GIN (to_tsvector('english', coalesce(content, '') || ' ' || coalesce(title, '')));
CREATE INDEX IF NOT EXISTS idx_jobs_status_created
    ON jobs (status, created_at);
```