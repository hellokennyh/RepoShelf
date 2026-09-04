-- RepoShelf Phase 1: Initial Database Schema
-- favorites table
CREATE TABLE favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_repo_id INTEGER UNIQUE,
    github_url TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    category TEXT NOT NULL DEFAULT 'Other',
    stars INTEGER NOT NULL DEFAULT 0,
    license TEXT,
    pushed_at TEXT,
    rating INTEGER
        CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    notes TEXT NOT NULL DEFAULT '',
    favorited_at TEXT NOT NULL,
    metadata_refreshed_at TEXT NOT NULL
);

-- favorite_tags table with cascade delete on favorite deletion
CREATE TABLE favorite_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    favorite_id INTEGER NOT NULL,
    tag TEXT NOT NULL COLLATE NOCASE,
    source TEXT NOT NULL
        CHECK (source IN ('github', 'ai', 'user')),
    FOREIGN KEY (favorite_id)
        REFERENCES favorites(id)
        ON DELETE CASCADE,
    UNIQUE (favorite_id, tag, source)
);

-- selection_history table (no UNIQUE on github_url by design)
CREATE TABLE selection_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_repo_id INTEGER,
    github_url TEXT NOT NULL,
    selected_at TEXT NOT NULL
);

-- Required Indexes
CREATE INDEX idx_favorites_name ON favorites(name);
CREATE INDEX idx_favorites_category ON favorites(category);
CREATE INDEX idx_favorites_rating ON favorites(rating);
CREATE INDEX idx_favorites_stars ON favorites(stars);
CREATE INDEX idx_favorite_tags_tag ON favorite_tags(tag);
CREATE INDEX idx_selection_repo_id ON selection_history(github_repo_id);
CREATE INDEX idx_selection_url ON selection_history(github_url);
