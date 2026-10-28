-- What a point is on a routine's page: a fact about them (`about`), a note
-- on a thing on their shelf (`thing`), a step of a routine day (`step`) or
-- advice (`tip`). Cards and documents keep `note`s.
ALTER TABLE points ADD COLUMN kind text NOT NULL DEFAULT 'note';
