-- Notifications for being mentioned (`@Ada`) in a comment or description.
ALTER TABLE notifications
    DROP CONSTRAINT notifications_type_check,
    ADD CONSTRAINT notifications_type_check CHECK (type IN ('assigned', 'comment', 'completed', 'suggestion', 'mention'));
