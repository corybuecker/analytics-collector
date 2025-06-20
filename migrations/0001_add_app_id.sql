ALTER TABLE events
    ADD COLUMN app_id TEXT;

UPDATE events
SET app_id = ''
WHERE app_id IS NULL;

ALTER TABLE events
    ALTER COLUMN app_id SET NOT NULL;