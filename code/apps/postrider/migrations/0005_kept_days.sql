-- The last day read from a calendar server, so today still shows when the
-- server can't be reached.
CREATE TABLE kept_days (
    calendar_id uuid PRIMARY KEY REFERENCES calendars (id) ON DELETE CASCADE,
    day_from timestamptz NOT NULL,
    day_to timestamptz NOT NULL,
    events jsonb NOT NULL,
    read_at timestamptz NOT NULL
);
