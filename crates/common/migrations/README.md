# Database Migrations

Migration SQL lives in `crates/migrations/migrations`. New files must also be
registered in `crates/migrations/src/lib.rs`; its tests enforce an exact match.

## Running Migrations

The easiest way to run migrations is to use the vendored CLI,
`circusctl migrate`. Packagers should vendor this crate if possible.

```bash
# Run all pending migrations
circusctl migrate up postgresql://user:password@localhost/circus

# Validate current schema
circusctl migrate validate postgresql://user:password@localhost/circus

# Create a new migration
circusctl migrate create migration_name
```
