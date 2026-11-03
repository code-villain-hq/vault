-- Raven clears out calls, logs and checks older than 30 days every hour. These
-- let it find them by time alone, a batch at a time, instead of reading
-- every row.
CREATE INDEX spans_started_at ON spans (started_at);
CREATE INDEX logs_at ON logs (at);
CREATE INDEX health_checks_checked_at ON health_checks (checked_at);
