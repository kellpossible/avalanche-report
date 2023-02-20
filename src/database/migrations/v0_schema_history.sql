BEGIN;
/* Previously using refinery crate for migrations, this emulates
 its table creation for new databases, ready for the next migration. */
CREATE TABLE IF NOT EXISTS refinery_schema_history (
    version INTEGER PRIMARY KEY,
    name TEXT,
    applied_on TEXT,
    checksum TEXT
);

CREATE TABLE schema_history (
    version INTEGER PRIMARY KEY,
    name TEXT,
    applied_on TEXT,
    checksum TEXT
);
COMMIT;
