# Database Migrations

This directory contains SQL migrations for the circus database.

## Migration Files

- `0001_schema.sql`: Full schema, all tables, indexes, triggers, and views.
- `0002_example.sql`: Example stub for the next migration when we make a stable
  release.

## Running Migrations

The easiest way to run migrations is to use the vendored CLI, `circus-migrate`.
Packagers should vendor this crate if possible.

```bash
# Run all pending migrations
circus-migrate up postgresql://user:password@localhost/circus

# Validate current schema
circus-migrate validate postgresql://user:password@localhost/circus

# Create a new migration
circus-migrate create migration_name
```
