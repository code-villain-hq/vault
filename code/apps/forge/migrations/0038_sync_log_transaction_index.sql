-- A spec's history finds the spec change applied in the same transaction
-- as each of its versions.
CREATE INDEX sync_log_transaction_idx ON sync_log (transaction_id);
