# Installing `cav` on Windows

The install script is POSIX shell, so on Windows use one of the options below.

## Manual download (recommended)

1. Download `cav-x86_64-pc-windows-msvc.exe` from the
   [releases page](https://github.com/orelvis15/cavs-hub-cli/releases).
2. Rename it to `cav.exe` and place it in a folder on your `PATH`, e.g.
   `%LOCALAPPDATA%\Programs\cav\`.
3. Add that folder to your `PATH` (PowerShell, current user):
   ```powershell
   $dir = "$env:LOCALAPPDATA\Programs\cav"
   [Environment]::SetEnvironmentVariable(
     "Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$dir", "User")
   ```
4. Open a new terminal so the `PATH` change takes effect.

You also need `cavs-lfs-agent.exe` (from the
[`cavs-oss`](https://github.com/orelvis15/cavs-oss) releases) on your `PATH` for
`cav install-lfs` / `cav repo connect` to wire up Git LFS.

## Scoop / winget

> Planned. Once published:
>
> ```powershell
> scoop install cav
> # or
> winget install cav
> ```

## From source

Install the [Rust toolchain](https://rustup.rs) (MSVC), then:

```powershell
git clone https://github.com/orelvis15/cavs-hub-cli
cd cavs-hub-cli
cargo build --release
# binary at target\release\cav.exe
```

See [install_source.md](./install_source.md) for details.

## Requirements

- [Git for Windows](https://git-scm.com/download/win) and Git LFS.

## Verify

```powershell
cav --version
cav status
```

The config file lives at `%USERPROFILE%\.config\cav\config.toml` (or
`%XDG_CONFIG_HOME%\cav\config.toml` if that variable is set).
