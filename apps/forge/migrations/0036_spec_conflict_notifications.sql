-- People hear when another open issue's spec change wants to change a
-- requirement their issue changes first (decision "A spec change is written
-- first, and only one issue changes a requirement at a time").
ALTER TABLE notifications
    DROP CONSTRAINT notifications_type_check,
    ADD CONSTRAINT notifications_type_check CHECK (type IN ('assigned', 'comment', 'completed', 'suggestion', 'mention', 'spec_conflict'));
