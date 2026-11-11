-- API parity additions (notes/settings/search/transformations support)

CREATE TABLE IF NOT EXISTS transformations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    title VARCHAR NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    prompt TEXT NOT NULL DEFAULT '',
    apply_default BOOLEAN NOT NULL DEFAULT FALSE,
    model_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- default prompt/settings stored in settings(key,value jsonb)
-- search uses existing FTS indexes from init
