PRAGMA foreign_keys=OFF;

-- Migrate build_info

CREATE TABLE new_build_info (
    "nightly" VARCHAR NOT NULL,
    "target" VARCHAR NOT NULL,
    "status" VARCHAR NOT NULL,
    "stderr" VARCHAR NOT NULL,
    "mode" VARCHAR NOT NULL,

    PRIMARY KEY ("nightly", "target", "mode")
);

INSERT INTO new_build_info (nightly, target, status, stderr, mode)
SELECT nightly, target, status, stderr, 'core' FROM build_info;

DROP TABLE build_info;

ALTER TABLE new_build_info RENAME TO build_info;


-- Migrate finished_nightly

CREATE TABLE new_finished_nightly (
    "nightly" VARCHAR NOT NULL,
    "mode" VARCHAR NOT NULL,

    PRIMARY KEY ("nightly", "mode")
);

INSERT INTO new_finished_nightly (nightly, mode)
SELECT nightly, 'core' FROM finished_nightly;

DROP TABLE finished_nightly;

ALTER TABLE new_finished_nightly RENAME TO finished_nightly;

-- Finish

PRAGMA foreign_keys=ON;


