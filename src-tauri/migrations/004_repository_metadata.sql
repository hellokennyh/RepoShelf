-- Migration 004: Add releases count and languages breakdown to favorites
ALTER TABLE favorites ADD COLUMN releases INTEGER;
ALTER TABLE favorites ADD COLUMN languages TEXT;
