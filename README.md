# cavs-hub-cli

The **CAVS Hub** command-line client — the `cav` command. See
[Install](#install) below for the one-line install.

It is a thin client for the CAVS Hub control plane (the Go API in the `cavshub`
repo). It authenticates with a CAVS access token, connects a local Git
repository to a Hub repository, and wires Git LFS up to the CAVS custom transfer
agent (`cavs-lfs-agent`, from the [`cavs-oss`](https://github.com/orelvis15/cavs-oss)
repo) so only content-defined-chunked, deduplicated bytes travel on push/pull.

## Install

```sh
# Prebuilt — installs the latest release for your platform from GitHub Releases:
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh

# Pin a specific version:
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | CAVS_VERSION=0.1.0 sh

# From source (this repo). Builds `cav`, and `cavs-lfs-agent` from a sibling
# cavs-oss checkout if present:
./build-install.sh
```

## Uninstall

```sh
# Removes cav and cavs-lfs-agent from your PATH:
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/uninstall.sh | sh

# Also delete the config (~/.config/cav):
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/uninstall.sh | CAVS_PURGE=1 sh
```

Set `CAVS_INSTALL_DIR` if you installed to a custom directory. To remove by
hand: `rm "$(command -v cav)" "$(command -v cavs-lfs-agent)"` and delete
`~/.config/cav`.

## Commands

| Command | What it does |
| --- | --- |
| `cav login` | Store & validate a `cavs_…` access token (created in the dashboard → Tokens). |
| `cav logout` | Remove the stored credentials. |
| `cav whoami` | Show the authenticated identity and its organizations. |
| `cav repo connect <org>/<repo>` | Point the current repo's `lfs.url` at CAVS and wire the transfer agent. |
| `cav install-lfs` | Configure just the LFS transfer agent for the current repo. |
| `cav status` | Print the config path, API base and login state (no network). |
| `cav update` | Update `cav` in place to the latest published release (`--check` to only report). |

## Configuration

Config lives at `$XDG_CONFIG_HOME/cav/config.toml` (default
`~/.config/cav/config.toml`), written `0600`. The API base is resolved as:

1. `--api <url>` flag
2. `$CAVS_API`
3. the stored config
4. the built-in default (`https://api.cavscloud.com`)

For local development against the dev stack:

```sh
export CAVS_API=http://localhost:8080
cargo run -- login --token cavs_pat_xxx
cargo run -- repo connect acme/game-assets
```

## Build

```sh
cargo build --release   # produces target/release/cav
```

## Staying up to date

- **Self-update:** `cav update` downloads the latest release asset for your
  platform and replaces the running binary. `cav update --check` only reports.
- **Automatic reminder:** any command checks GitHub for a newer release at
  most once per day and prints a one-line warning to stderr if one exists
  (never blocks). Throttled via `~/.config/cav/update_check.toml`. Opt out
  with `CAV_NO_UPDATE_CHECK=1`; set `GITHUB_TOKEN` to avoid API rate limits.

## Auth model

The API accepts `Authorization: Bearer <token>` where the token is either a
Firebase ID token (used by the web app) or a CAVS access token prefixed
`cavs_` (PAT / REPO / CI). The CLI uses the latter: create one in the dashboard,
paste it into `cav login`. Personal access tokens act as your user bounded by
their scopes, so `whoami`, listing repos and `repo connect` all work.

## Relationship to other repos

- **`cavs-oss`** — the open-source CAVS core (Rust): chunking, store, formats,
  and the `cavs-lfs-agent` transfer agent this CLI wires into Git.
- **`cavshub`** — the CAVS Hub control plane (Go API + React dashboard) this CLI
  authenticates against.

## Documentation

- Per-platform install guides: [`docs/`](./docs/README.md)
- Contributing: [`.github/CONTRIBUTING.md`](./.github/CONTRIBUTING.md)
- Security policy: [`.github/SECURITY.md`](./.github/SECURITY.md)
- Code of Conduct: [`.github/CODE-OF-CONDUCT.md`](./.github/CODE-OF-CONDUCT.md)

## License

Released under the [MIT License](./LICENSE).
