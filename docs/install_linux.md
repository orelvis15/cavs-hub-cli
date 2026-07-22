# Installing `cav` on Linux

## Install script (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh
```

This detects your architecture (`x86_64` or `aarch64`) and installs `cav` and
`cavs-lfs-agent` into `/usr/local/bin` when writable, otherwise `~/.local/bin`.

If the target directory is not on your `PATH`, the script prints the line to add
to your shell profile, e.g.:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.profile
```

Override the destination with `CAVS_INSTALL_DIR`:

```sh
curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | CAVS_INSTALL_DIR=/usr/local/bin sh
```

## Manual download

1. Download the Linux build for your architecture from the
   [releases page](https://github.com/orelvis15/cavs-hub-cli/releases):
   - `cav-x86_64-unknown-linux-gnu`
   - `cav-aarch64-unknown-linux-gnu`
2. Install it onto your `PATH`:
   ```sh
   chmod +x cav-*-unknown-linux-gnu
   sudo install -m 0755 cav-*-unknown-linux-gnu /usr/local/bin/cav
   ```

## Requirements

- `git` and `git-lfs` must be installed to use `cav repo connect` /
  `cav install-lfs`. On Debian/Ubuntu:
  ```sh
  sudo apt-get install git git-lfs
  ```
- The Linux builds are glibc-based. On musl distros (e.g. Alpine), build from
  source — see [install_source.md](./install_source.md).

## From source

See [install_source.md](./install_source.md).

## Verify

```sh
cav --version
cav status
```

## Upgrade / uninstall

- Upgrade: re-run the install script, or `cav update`.
- Uninstall:
  ```sh
  curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/uninstall.sh | sh
  ```
  Add `CAVS_PURGE=1` to also delete `~/.config/cav`. By hand:
  `rm "$(command -v cav)" "$(command -v cavs-lfs-agent)"` and remove
  `~/.config/cav`.
