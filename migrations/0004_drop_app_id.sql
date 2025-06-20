BEGIN;

UPDATE events
SET recorded_by = app_id
WHERE recorded_by IS NULL;

ALTER TABLE events
    DROP COLUMN app_id;

ALTER TABLE events
    ALTER COLUMN recorded_by SET NOT NULL;

COMMIT;