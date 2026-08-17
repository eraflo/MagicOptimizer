# Contributing

Thanks for looking. The project is early — see [Status](README.md#status) for what actually
exists.

**Read [`CLAUDE.md`](CLAUDE.md) first.** Despite the name it is not only for AI agents: it holds
the non-negotiable invariants and the known pitfalls, and it explains *why* several unusual
choices were made (no SQLite, nothing usable during a game, no committed data artifacts).

## Setup

You need a recent stable Rust toolchain. Nothing else for the core crates — every dependency is
pure Rust, and that is deliberate.

```bash
git clone https://github.com/eraflo/MagicOptimizer.git
cd MagicOptimizer
cargo test --workspace
```

For the desktop app (from phase 2) you will also need the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) and Node.js.

For Android (from phase 6), see [`docs/dev/android.md`](docs/dev/android.md).

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs exactly these three. A Clippy warning fails the build: fix it rather than silencing it
with `#[allow]`, and if silencing really is right, write the reason in a comment next to it.

## Conventions

- **Everything in English** — documentation, comments, identifiers, commit messages, UI strings.
- **Conventional commits**: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- **Errors**: `thiserror` in crates, `anyhow` only in binaries.
- **No `unwrap()` or `expect()` in domain crates.** Fine in tests and in binary startup code.
- **Never `panic!` on external data.** Card data comes from the network; treat it as untrusted.
- Keep domain logic in `crates/`. `src-tauri/` holds thin commands only — that separation is what
  lets the core be tested without a mobile build.

## Adding a dependency

Check that it pulls in **no C or C++**, or that it cross-compiles cleanly to
`aarch64-linux-android`. This is the single most important constraint in the project: the current
stack is pure Rust precisely so the Android build stays boring.
[`docs/dev/android.md`](docs/dev/android.md) explains what went into that decision.

## Scope

Feature ideas are welcome, with one standing exception: **anything meant to be used during a game
is out of scope** — life counters, rules lookup at the table, live draft pick assistance. The
reasoning is in [the FAQ](docs/user/faq.md#why-is-there-no-life-counter).

Note also that the project must stay free and non-commercial to remain within the Wizards Fan
Content Policy, so monetization features cannot be accepted.
