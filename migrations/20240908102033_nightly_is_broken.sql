ALTER TABLE finished_nightly
    ADD COLUMN is_broken BOOLEAN NOT NULL DEFAULT FALSE;