-- Add migration script here

CREATE TABLE build_info (
    "nightly" VARCHAR NOT NULL,
    "target" VARCHAR NOT NULL,
    "status" VARCHAR NOT NULL,
    "stderr" VARCHAR NOT NULL,

    PRIMARY KEY ("nightly", "target")
);

CREATE TABLE finished_nightly (
    "nightly" VARCHAR NOT NULL PRIMARY KEY
);
