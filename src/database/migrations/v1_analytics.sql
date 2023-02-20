CREATE TABLE IF NOT EXISTS analytics (
    id TEXT NOT NULL PRIMARY KEY,
    uri TEXT NOT NULL,
    visits INTEGER,
    time NUMERIC
);
