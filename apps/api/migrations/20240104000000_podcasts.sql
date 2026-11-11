-- Podcast profiles and episodes (partial REQ-011)

CREATE TABLE IF NOT EXISTS speaker_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL UNIQUE,
    description TEXT,
    voice_model VARCHAR,
    speakers JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS episode_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL UNIQUE,
    description TEXT,
    speaker_config VARCHAR, -- speaker_profile id string (or legacy name)
    outline_llm VARCHAR,
    transcript_llm VARCHAR,
    language VARCHAR,
    default_briefing TEXT NOT NULL DEFAULT '',
    num_segments INT NOT NULL DEFAULT 5,
    max_tokens INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS episodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    episode_profile JSONB NOT NULL DEFAULT '{}'::jsonb,
    speaker_profile JSONB NOT NULL DEFAULT '{}'::jsonb,
    briefing TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    notebook_id UUID,
    audio_file VARCHAR,
    transcript JSONB,
    outline JSONB,
    command_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_episode_profiles_name ON episode_profiles(name);
CREATE INDEX IF NOT EXISTS idx_speaker_profiles_name ON speaker_profiles(name);
CREATE INDEX IF NOT EXISTS idx_episodes_command_id ON episodes(command_id);
CREATE INDEX IF NOT EXISTS idx_episodes_created_at ON episodes(created_at DESC);
