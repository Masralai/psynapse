-- ponytail: FTS index for insight search parity/perf (text search over insights added in API).
CREATE INDEX IF NOT EXISTS idx_source_insights_content_tsv
    ON source_insights USING GIN (to_tsvector('english', coalesce(content, '') || ' ' || coalesce(insight_type, '')));
