-- Issue history reads one record's versions from the log.
CREATE INDEX sync_log_model_idx ON sync_log (model_id, sync_id);
