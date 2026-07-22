# Contributing to CAVS Hub CLI

Thanks for your interest in improving `cav`! This document explains how to build
the project, run the tests, and submit changes.

## Prerequisites

- [Rust](https://rustup.rs) (stable) with `rustfmt` and `clippy` components:
  ```sh
  rustup component add rustfmt clippy
  ```
- `git` (and `git-lfs` to exercise the LFS commands).
- Optional: a checkout of [`cavs-oss`](https://github.com/orelvis15/cavs-oss)
  next to this repo if you want to build the `cavs-lfs-agent` locally.

## Building

```sh
cargo build            # debug build at target/debug/cav
cargo build --release  # optimized build at target/release/cav
cargo run -- status    # run a subcommand without installing
```

To build and install locally (into `~/.cargo/bin`):

```sh
./build-install.sh
```

## Testing & checks

Before opening a pull request, make sure all of these pass — CI runs the same
checks:

```sh
cargo fmt --all -- --check   # formatting
cargo clippy --all-targets -- -D warnings   # lints (warnings are errors)
cargo test                   # unit tests
```

## Submitting a pull request

1. Open an issue first for anything beyond a small fix, so we can agree on the
   approach before you invest time.
2. Fork the repo and create a topic branch from `main`.
3. Make your change, add tests where it makes sense, and keep the diff focused.
4. Run the checks above.
5. Open the pull request. Reference the issue it addresses with `Fixes #NUMBER`
   in the description, and fill in the pull request template.

Keep pull requests scoped to a single concern — smaller PRs are reviewed faster.

## Commit messages

Write clear, imperative commit messages (e.g. "Add `cav logout` command"). Group
related work into logical commits; there is no strict convention beyond
readability.

## Design guidelines

`cav` is intentionally a **thin client**. Heavy lifting (content-defined
chunking, dedup) lives in `cavs-oss`; server-side logic lives in the `cavshub`
control plane. Prefer:

- clear, actionable error messages (users run this in their terminal);
- talking to the documented API rather than assuming internals;
- graceful behaviour when offline or unauthenticated.

## Reporting bugs & requesting features

Use the [issue templates](./ISSUE_TEMPLATE). For security issues, do **not**
open a public issue — see [SECURITY.md](./SECURITY.md).

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](../LICENSE).
