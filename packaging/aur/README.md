# AUR packaging

`PKGBUILD` for the `oom-tui-bin` AUR package. It installs the prebuilt musl
binary from the matching GitHub release and verifies it against the release's
published SHA-256 sums, so it needs no Rust toolchain to install.

## Publishing (one-time)

Requires an [AUR account](https://aur.archlinux.org) with an SSH key registered,
on a machine with `makepkg` (Arch, or an `archlinux` container):

```sh
makepkg --printsrcinfo > .SRCINFO      # AUR requires this; generated, not hand-written
git clone ssh://aur@aur.archlinux.org/oom-tui-bin.git aur-repo
cp PKGBUILD .SRCINFO aur-repo/
cd aur-repo && git add PKGBUILD .SRCINFO && git commit -m "oom-tui-bin 0.4.0" && git push
```

Test locally first with `makepkg -si` (installs), `oom-tui --version`, then
`sudo pacman -R oom-tui-bin` (removes).

## Updating for each release

1. Bump `pkgver` and reset `pkgrel=1`.
2. Replace both `sha256sums_*` with the new release's published sums:
   ```sh
   for a in x86_64 aarch64; do
     curl -fsSL "https://github.com/Ashfaaq98/oom-tui/releases/download/v<VER>/oom-tui-$a-unknown-linux-musl.tar.gz.sha256"
   done
   ```
3. Regenerate `.SRCINFO` and push (as above).
