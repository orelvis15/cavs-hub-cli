# CAVS Hub CLI documentation

Internal documentation for the `cav` command line tool.

## Installation

Pick your platform:

- [macOS](./install_macos.md)
- [Linux](./install_linux.md)
- [Windows](./install_windows.md)
- [From source](./install_source.md) (any platform with a Rust toolchain)

The quickest path on macOS/Linux is the install script:

```sh
curl -fsSL https://cavscloud.com/install.sh | sh
```

It installs two binaries:

| Binary | Purpose |
| --- | --- |
| `cav` | the CAVS Hub CLI (this repo) |
| `cavs-lfs-agent` | the Git LFS custom transfer agent (from [`cavs-oss`](https://github.com/orelvis15/cavs-oss)) |

## After installing

```sh
cav status                       # config path, API base, login state
cav login                        # paste a cavs_ access token from the dashboard
cav repo connect <org>/<repo>    # run inside your Git repo
git push
```

See the [top-level README](../README.md) for the full command reference and the
authentication model.
