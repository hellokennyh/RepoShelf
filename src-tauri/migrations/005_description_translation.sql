-- Migration 005: Add description translation and source hash to favorites
ALTER TABLE favorites ADD COLUMN description_translations TEXT;
ALTER TABLE favorites ADD COLUMN description_translation_source_hash TEXT;
