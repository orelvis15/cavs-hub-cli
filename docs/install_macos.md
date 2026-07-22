# Installing `cav` on macOS

## Install script (recommended)

```sh
curl -fsSL https://cavscloud.com/install.sh | sh
```

This detects your architecture (Apple Silicon `arm64` or Intel `x86_64`) and
installs `cav` and `cavs-lfs-agent` into `/usr/local/bin` (or `~/.local/bin`
when `/usr/local/bin` is not writable).

Override the destination with `CAVS_INSTALL_DIR`:

```sh
curl -fsSL https://cavscloud.com/install.sh | CAVS_INSTALL_DIR="$HOME/bin" sh
```

## Homebrew

> Planned. Once the tap is published:
>
> ```sh
> brew install orelvis15/tap/cav
> ```

## Manual download

1. Download the macOS build for your architecture from the
   [releases page](https://github.com/orelvis15/cavs-hub-cli/releases):
   - Apple Silicon: `cav-aarch64-apple-darwin`
   - Intel: `cav-x86_64-apple-darwin`
2. Make it executable and move it onto your `PATH`:
   ```sh
   chmod +x cav-*-apple-darwin
   sudo mv cav-*-apple-darwin /usr/local/bin/cav
   ```
3. If macOS Gatekeeper blocks the unsigned binary, allow it once:
   ```sh
   xattr -d com.apple.quarantine /usr/local/bin/cav
   ```

## From source

See [install_source.md](./install_source.md).

## Verify

```sh
cav --version
cav status
```

## Upgrade / uninstall

- Upgrade: re-run the install script (it overwrites the existing binary).
- Uninstall: `rm "$(command -v cav)" "$(command -v cavs-lfs-agent)"` and
  remove `~/.config/cav`.
