-- Add migration script here

ALTER TABLE notification_issues
    ADD COLUMN "last_update_date" INTEGER NULL;
