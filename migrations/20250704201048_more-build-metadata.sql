-- Add migration script here

ALTER TABLE "build_info"
    ADD COLUMN "build_date" INTEGER NULL;

ALTER TABLE "build_info"
    ADD COLUMN "build_duration_ms" INTEGER NULL;

ALTER TABLE "build_info"
    ADD COLUMN "does_it_build_version" VARCHAR NULL;
