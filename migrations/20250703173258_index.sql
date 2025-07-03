-- Add migration script here

CREATE INDEX IF NOT EXISTS build_info_nightly ON build_info (nightly);

CREATE INDEX IF NOT EXISTS build_info_target ON build_info (target);
