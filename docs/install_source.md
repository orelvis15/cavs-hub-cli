# Installing `cav` from source

Works on any platform with a Rust toolchain. This is the recommended path on
musl Linux, uncommon architectures, or when you want to track `main`.

## Prerequisites

- [Rust](https://rustup.rs) (stable). The repo pins `edition = "2021"`; a recent
  stable toolchain is fine.
- `git`, and `git-lfs` if you plan to use the LFS commands.

## Option A — `build-install.sh` (recommended)

Builds and installs `cav`, and `cavs-lfs-agent` from a sibling `cavs-oss`
checkout when one is present:

```sh
git clone https://github.com/orelvis15/cavs-hub-cli
cd cavs-hub-cli
./build-install.sh
```

Useful flags / environment:

| Flag / env | Effect |
| --- | --- |
| `--dir <path>` / `CAVS_INSTALL_DIR` | Install into a specific directory instead of `~/.cargo/bin`. |
| `--no-agent` | Skip building/installing `cavs-lfs-agent`. |
| `CAVS_OSS_DIR` | Path to the `cavs-oss` checkout (default: `../cavs-oss`). |

## Option B — `cargo install`

```sh
cargo install --git https://github.com/orelvis15/cavs-hub-cli
```

or from a local checkout:

```sh
cargo install --path .
```

Both install the `cav` binary into `~/.cargo/bin` (make sure it is on your
`PATH`). Install the LFS agent separately from `cavs-oss`:

```sh
cargo install --path ../cavs-oss/core/cavs-lfs-agent
```

## Option C — plain build

```sh
cargo build --release      # produces target/release/cav
```

## Verify

```sh
cav --version
cav status
```
