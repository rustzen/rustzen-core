# Rustzen Core Agent Rules

## Repository Purpose

This repository contains product-neutral Rust primitives shared by Rustzen products.

## Boundaries

- Keep business schemas, product queries, UI code, release signing, and updater logic out of this repository.
- Prefer small crates with explicit inputs and no hidden globals.
- Do not add new crate families without first fitting the need into one of the existing crates: `rz-core`, `rz-cli`, `rz-config`, `rz-fs`, or `rz-platform`.
- Keep product-specific defaults in product repositories unless at least two products use the same behavior.

## Commands

- `just check`
- `cargo test --workspace --all-features`
- `cargo fmt --all -- --check`

## Change Style

- Add or change one shared capability at a time.
- Keep APIs stable, explicit, and easy for AI agents to call.
- Add tests for each helper that has branching behavior.
