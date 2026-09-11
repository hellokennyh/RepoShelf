-- Migration 002: Add rating_reason and rating_confidence to favorites
ALTER TABLE favorites ADD COLUMN rating_reason TEXT;
ALTER TABLE favorites ADD COLUMN rating_confidence TEXT CHECK (rating_confidence IS NULL OR rating_confidence IN ('low', 'medium', 'high'));
