# Contributing

Thanks for your interest in `clashai-environments`. Contributions are welcome.

## Quick start

1. **Fork** this repo and clone your fork.
2. **Branch** off `main`: `git checkout -b feat/your-change`.
3. **Set up the toolchain** — `rust-toolchain.toml` pins stable. `rustup` will pick it up automatically.
4. **Make your change.** See `README.md` for the project layout and `PROTOCOL.md` for the wire contract.
5. **Run the local checks** that mirror CI (see below) — `git push` and open a PR against `main` only after they pass.

## Local checks

CI runs the following on every PR. Run them locally first to avoid round-trips:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit              # cargo install cargo-audit --locked, if not already installed
```

## Commit style

Conventional commits are encouraged but not enforced. Examples used in this repo:

- `feat(minimal-client): add --players flag`
- `fix(poker): legal_actions must return JSON array per PROTOCOL.md`
- `docs(coup): fix Wikipedia link`
- `chore(deps): bump rustls-webpki`

Keep commit bodies explanatory — describe *why* a change exists, not just *what* changed. Future readers (and `git blame`) will thank you.

## Adding an environment

See the "Adding an environment" section of `README.md`. Briefly: a new game needs a `*-protocol` crate (action/state types + `pub const X_RULES`), an optional `*-engine` crate, an `Environment` impl in `environment-engine`, and an optional viewer HTML in `services/environment-server/static/viewer/`.

## Tests

Each game has integration tests in `tests/<name>.rs` of its protocol crate. Inline `#[cfg(test)] mod tests` blocks inside `src/` are discouraged — keep tests in the dedicated test directory so the public API surface is exercised.

## Reporting bugs

For functional bugs: open a regular GitHub issue with reproduction steps.

For security issues: see [`SECURITY.md`](SECURITY.md). **Do not** file public issues for vulnerabilities.

## Code of conduct

By participating you agree to abide by the [Contributor Covenant](CODE_OF_CONDUCT.md).

## Attribution

Contributors are listed in [`CONTRIBUTORS.md`](CONTRIBUTORS.md). After your first merged PR, feel free to add yourself in alphabetical order in the same PR or a follow-up.
