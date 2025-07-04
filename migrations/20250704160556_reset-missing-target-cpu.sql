-- Add migration script here

ALTER TABLE build_info
    ADD COLUMN rustflags VARCHAR;

DELETE FROM build_info WHERE target IN ('avr-none', 'amdgcn-amd-amdhsa') AND mode = 'core';

DELETE FROM finished_nightly WHERE nightly > '2025-02-11';
