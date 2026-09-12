-- Migration 003: Clean up duplicate AI tags that already exist as GitHub tags for the same favorite
DELETE FROM favorite_tags
WHERE source = 'ai'
  AND EXISTS (
      SELECT 1 FROM favorite_tags g
      WHERE g.favorite_id = favorite_tags.favorite_id
        AND g.source = 'github'
        AND trim(g.tag) = trim(favorite_tags.tag) COLLATE NOCASE
  );
