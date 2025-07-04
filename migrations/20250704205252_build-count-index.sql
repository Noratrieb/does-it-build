-- Add migration script here

CREATE INDEX IF NOT EXISTS build_info_status ON build_info (status);
