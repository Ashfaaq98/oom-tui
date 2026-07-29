# Installing OOM-TUI

OOM-TUI provides static Linux release binaries for `x86_64` and `aarch64`.
The release installer downloads the matching archive and verifies its SHA-256
checksum before installing the binary.

## Standard user install

This installs to `~/.local/bin` and does not need `sudo`:

```bash
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh
```

Make sure `~/.local/bin` is on your `PATH`, then confirm the result:

```bash
oom-tui --version
```

## Update, uninstall, and system install

Use the same installer with an option:

```bash
# Reinstall the latest release in the current user's ~/.local/bin
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh -s -- --update

# Install for all users in /usr/local/bin (prompts for sudo when needed)
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh -s -- --system

# Remove the user installation
curl -fsSL https://github.com/Ashfaaq98/oom-tui/releases/latest/download/install.sh | sh -s -- --uninstall
```

Add `--system` to the uninstall command to remove `/usr/local/bin/oom-tui`.

## Install a specific release

Replace `vX.Y.Z` with the release tag in both places:

```bash
version=vX.Y.Z
curl -fsSL "https://github.com/Ashfaaq98/oom-tui/releases/download/${version}/install.sh" | sh -s -- --version "$version"
```

## Manual verified install

Download the archive and checksum for your architecture, then verify before
extracting. For `aarch64`, replace the target below accordingly.

```bash
target=x86_64-unknown-linux-musl
base="https://github.com/Ashfaaq98/oom-tui/releases/latest/download/oom-tui-${target}.tar.gz"
curl -fLO "$base"
curl -fLO "$base.sha256"
sha256sum -c "$(basename "$base").sha256"
tar xzf "$(basename "$base")"
sudo install oom-tui-*/oom-tui /usr/local/bin/oom-tui
```

## Build from source

The minimum supported Rust version is 1.75.

```bash
git clone https://github.com/Ashfaaq98/oom-tui
cd oom-tui
cargo build --release
./target/release/oom-tui --help
```

When developing from a checkout, use `cargo run` so you execute that checkout
rather than an older installed binary:

```bash
cargo run -- --file examples/sample-oom.log
```

## Installer prerequisites and supported systems

The installer requires Linux, `curl`, `sha256sum`, and `tar`. It supports
`x86_64`/`amd64` and `aarch64`/`arm64`. The static binaries are intended to
run on mainstream glibc and musl Linux distributions; package-manager
installation options are tracked in the project roadmap.
