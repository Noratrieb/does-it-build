-- Add migration script here

ALTER TABLE finished_nightly
    ADD COLUMN broken_error BOOLEAN DEFAULT NULL;
