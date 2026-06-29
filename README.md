# Rustzen Core

Rustzen Core is the future home for shared, reusable Rustzen capabilities.

This repository is intentionally small today. It will be used when common
runtime, data, security, and platform patterns become stable enough to extract
from individual Rustzen products.

## Planned Scope

- Permission and access-control primitives.
- SQLite performance and maintenance utilities.
- Shared application configuration patterns.
- Common data-layer helpers and migration conventions.
- Cross-product runtime contracts that should stay consistent.

## Non-Goals

- Product-specific UI, packaging, updater, or release logic.
- One-off experiments that have not proven reuse across multiple products.
- Application code that belongs in a specific Rustzen product repository.

## Extraction Rule

Move code here only after at least two Rustzen products need the same behavior
and the ownership boundary is clear. Until then, keep product-specific code in
the product repository.

## Status

Planning placeholder. No reusable package has been published from this
repository yet.
