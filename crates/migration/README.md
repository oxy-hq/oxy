# Running Migrator CLI

The server applies pending migrations on startup, so running this CLI by hand is
only needed to generate a migration, or to drive the database somewhere the
server would not take it (a rollback, a `fresh`, a status check). It targets
whatever `OXY_DATABASE_URL` points at, falling back to the embedded development
PostgreSQL.

If you are running this in the root workspace, you will need to suffix all command with `-p migration` to run:

```sh
cargo run -p migration -- [OPTIONS]
```

- Generate a new migration file

  ```sh
  cargo run -- generate MIGRATION_NAME
  ```

- Apply all pending migrations

  ```sh
  cargo run -- up
  ```

- Apply first 10 pending migrations

  ```sh
  cargo run -- up -n 10
  ```

- Rollback last applied migrations

  ```sh
  cargo run -- down
  ```

- Rollback last 10 applied migrations

  ```sh
  cargo run -- down -n 10
  ```

- Drop all tables from the database, then reapply all migrations

  ```sh
  cargo run -- fresh
  ```

- Rollback all applied migrations, then reapply all migrations

  ```sh
  cargo run -- refresh
  ```

- Rollback all applied migrations

  ```sh
  cargo run -- reset
  ```

- Check the status of all migrations

  ```sh
  cargo run -- status
  ```
