DELETE FROM analytics
WHERE visits IS NULL OR time IS NULL;

ALTER TABLE analytics
ADD COLUMN new_visits INTEGER NOT NULL DEFAULT '';

ALTER TABLE analytics
ADD COLUMN new_time NUMERIC NOT NULL DEFAULT '';

UPDATE analytics
SET new_visits = visits;

UPDATE analytics
SET new_time = time;

ALTER TABLE analytics
DROP COLUMN visits;
ALTER TABLE analytics
DROP COLUMN time;

ALTER TABLE analytics
RENAME COLUMN new_visits TO visits;
ALTER TABLE analytics
RENAME COLUMN new_time TO time;

