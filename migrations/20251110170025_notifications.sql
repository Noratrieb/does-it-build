-- Add migration script here

CREATE TABLE notification_issues(
    "issue_number" INTEGER PRIMARY KEY,
    "status" TEXT NOT NULL, -- open/closed
    "first_failed_nightly" TEXT NOT NULL,
    "target" TEXT NOT NULL
) STRICT;

CREATE INDEX notification_issues_target on notification_issues("target", "status");
