# Steps to add support for a new database version:

- Add a new enum variant for new database version
- Add version transition for each DBVersion in `migrate_db` method.
- Add steps required for new migration in `migrate` method. In each migration
  step, you can either do in place migration or use temp_db/ to migrate data from
  existing db but finally it must atomically rename temp_db/ back to existing db
  name.
- Update `LATEST_DB_VERSION` to latest database version.
