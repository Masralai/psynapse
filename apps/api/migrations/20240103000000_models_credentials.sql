-- Models and credentials (API parity foundation)

CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    provider VARCHAR NOT NULL,
    modalities JSONB NOT NULL DEFAULT '[]'::jsonb,
    config JSONB NOT NULL DEFAULT '{}'::jsonb, -- ponytail: stores provider config incl secrets; encryption-at-rest hardening pending.
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS models (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    provider VARCHAR NOT NULL,
    type VARCHAR NOT NULL,
    credential_id UUID REFERENCES credentials(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_models_provider_type ON models(provider, type);
CREATE INDEX IF NOT EXISTS idx_credentials_provider ON credentials(provider);
