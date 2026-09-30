-- Migration 006: Add repository created_at and ai_analysis columns to favorites table
-- Exactly two columns added for RepoShelf v1.1.2 S1

ALTER TABLE favorites ADD COLUMN repo_created_at TEXT NULL;
ALTER TABLE favorites ADD COLUMN ai_analysis TEXT NULL;
