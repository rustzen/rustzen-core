# @formatter:off
# prettier-ignore
# justfile - Rustzen Core workspace commands

check:
    cargo fmt --all -- --check
    cargo test --workspace --all-features

fmt:
    cargo fmt --all

test:
    cargo test --workspace --all-features
