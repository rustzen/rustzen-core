# Rustzen Core

Rustzen Core is the shared Rust capability workspace for Rustzen products.

It contains executable reusable crates instead of only planning notes. Code here must stay product-neutral and must be useful to at least two Rustzen products before it is moved in.

## Crates

| Crate | Scope |
|---|---|
| `rz-core` | API envelopes, shared error primitives, stable hashing, built-in role policy helpers, deploy artifact validation, optional SQLite helpers, optional SQLite maintenance, and optional tracing/logging helpers. |
| `rz-config` | Runtime directory layout, primitive environment parsing, and `app.env` parsing/rendering. |
| `rz-fs` | Filesystem walking, size/count stats, safe removal, directory creation, path containment, and copy helpers. |
| `rz-cli` | CLI output mode, verbosity, top-level command error handling, JSON config discovery, and toggle resolution. |
| `rz-platform` | `/opt/rustzen-*` service layout, systemd service rendering, resource limits, and deployment plan rendering. |

## Toolchain

`rustzen-core` uses Rust `1.95.0`. The minimum package `rust-version` is
`1.94` because shared SQLite helpers are aligned with `sqlx 0.9.0`.

## Standards

- Keep product schemas, business queries, UI, updater logic, and release signing in product repositories.
- Put shared primitives here only when the behavior is needed by multiple Rustzen products.
- Prefer explicit configuration over hidden globals.
- Keep crates small and independently consumable.
- Keep SQLite helpers aligned with the Rustzen server stack on `sqlx 0.9.0`.
- Use `docs/standards.md` as the shared capability boundary reference.

## Current Consumers

Expected consumers include `rustzen-clear`, `rustzen-admin`, `rustzen-analytics`, `rustzen-inspect`, `rustzen-report`, `rustzen-clipboard`, `rustzen-zipper`, and `rustzen-video`.

No product repository is migrated automatically by this repository. Consumers should adopt these crates incrementally.

Current validated adoption:

| Product | Core revision | Adopted crates |
|---|---|---|
| `rustzen-admin` | `a7d706f0bd1e0af8021b00021916e5e02d7caa5c` | `rz-core` role policy, capability matching, SQLite, logging, and deploy artifact helpers; `rz-config` runtime layout; `rz-platform` build-time deployment rendering; `rz-fs` system status filesystem statistics. |

`rustzen-admin` does not consume `rz-cli` because it does not currently expose a Rust CLI command surface.
