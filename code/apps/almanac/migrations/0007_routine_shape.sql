-- A topic's page may be a routine, and a point says what it is on it.
ALTER TABLE topics DROP CONSTRAINT topics_shape_check;
ALTER TABLE topics ADD CONSTRAINT topics_shape_check
    CHECK (shape IN ('cards', 'document', 'routine'));
ALTER TABLE points ADD CONSTRAINT points_kind_check
    CHECK (kind IN ('note', 'about', 'thing', 'step', 'tip'));
