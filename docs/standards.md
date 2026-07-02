# Rustzen Shared Capability Standards

## Goal

`rustzen-core` provides shared Rust primitives for Rustzen products. It is not an application template and it must not absorb product-specific business logic.

## Crate Boundaries

### `rz-core`

Owns product-neutral primitives:

- API success and error envelopes.
- Framework-neutral HTTP error mapping.
- Common error type for shared helpers.
- Stable hashing helpers.
- Product-neutral built-in role policy classification for `owner`, `admin`, and `viewer`.
- Product-neutral capability grant matching for exact, wildcard, and
  colon-prefix wildcard grants.
- Product-neutral deploy artifact validation for binary markers, web dist zip
  structure, content hashes, signature markers, and binary architecture
  detection.
- SQLite URL/path, pool, tuning, migration, connection test, checkpoint, vacuum, optimize, and pragma snapshot helpers aligned on `sqlx 0.9.0`.
- Tracing/logging initialization for stdout, append-only file targets, daily rolling file targets, and date-based retention cleanup.

SQLite and logging helpers are optional crate features. Lightweight consumers
can depend on `rz-core` with `default-features = false` to use policy and
small primitives without pulling runtime/database dependencies.

Does not own product database schemas, business queries, auth business rules,
role persistence, menu persistence, product-specific error variants, or
localized messages.

### `rz-config`

Owns runtime and environment conventions:

- Runtime root layout.
- `data`, `data/db`, `logs`, `web/dist`, `uploads`, and `avatars` paths.
- Relative path resolution under runtime root.
- Primitive env parsing.
- `app.env` / `.env` parsing and rendering.

Product repositories compose their own config structs using these primitives.

### `rz-fs`

Owns filesystem primitives:

- Recursive path statistics.
- File or directory removal.
- Directory creation.
- Parent directory creation.
- Copy-if-missing and copy-if-different helpers.
- Canonical path containment checks.

Product-specific scan rules, archive filters, cleanup policy, and safety allowlists stay local.

### `rz-cli`

Owns CLI conventions:

- Text/JSON output mode.
- Quiet/normal/verbose behavior.
- Toggle resolution for paired positive/negative flags.
- Top-level command error printing.
- Config discovery from explicit path, `.rzrc`, `.rzrc.json`, and `package.json` field.

Product subcommands and command-specific validation stay local.

### `rz-platform`

Owns platform conventions:

- `/opt/rustzen-*` install layout.
- `config/app.env`, `bin`, `data/db`, `logs`, `systemd`, and `web/dist` paths.
- systemd service rendering.
- Resource limit rendering.
- Deployment plan rendering for service files, env files, and required directories.

Release signing, updater flow, Docker base images, package extraction, and product packaging policy stay local.

## Adoption Rule

A product repository should adopt one helper at a time:

1. Replace local duplicate helper with the matching `rz-*` crate function.
2. Keep the current product behavior explicit and covered by product regression tests.
3. Add a product-level regression test around the replaced behavior.
4. Do not move business-specific structures into `rustzen-core`.

## Runtime Dependency Baseline

- Rust toolchain: `1.95.0`.
- Minimum Rust package version: `1.94`.
- SQLite dependency: `sqlx 0.9.0`.
- Shared logging dependency: `tracing`, `tracing-subscriber`, and
  `tracing-appender`.

## Initial Migration Order

1. SQLite helpers and daily runtime logging from `rustzen-admin`,
   `rustzen-analytics`, `rustzen-inspect`, `rustzen-report`, and
   `rustzen-clipboard` into `rz-core`.
2. Runtime layout and env parsing from `rustzen-admin`, `rustzen-analytics`, `rustzen-report`, and `rustzen-clipboard` into `rz-config`.
3. systemd and install layout conventions from server products into `rz-platform`.
4. CLI output and config-file discovery from `rustzen-clear` and `rustzen-zipper` into `rz-cli`.
5. Filesystem stats, remove, and containment helpers from `rustzen-clear`, `rustzen-zipper`, and `rustzen-clipboard` into `rz-fs`.
