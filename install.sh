#!/bin/sh
# Install oom-tui from a GitHub release. Linux only.

set -eu

REPOSITORY="Ashfaaq98/oom-tui"
RELEASE_BASE_URL="${OOM_TUI_RELEASE_BASE_URL:-https://github.com/${REPOSITORY}/releases}"
USER_INSTALL_DIR="${OOM_TUI_INSTALL_DIR:-${HOME}/.local/bin}"
SYSTEM_INSTALL_DIR="${OOM_TUI_SYSTEM_DIR:-/usr/local/bin}"

release_version="latest"
mode="install"
system_install=false

usage() {
    cat <<'EOF'
Usage: install.sh [OPTIONS]

Install oom-tui from a GitHub release.

Options:
  --version <tag>  Install a specific release (for example, v0.2.0).
  --update         Install the latest release into the selected location.
  --uninstall      Remove oom-tui from the selected location.
  --system         Use /usr/local/bin instead of ~/.local/bin.
  -h, --help       Show this help text.
EOF
}

fail() {
    printf '%s\n' "oom-tui installer: $*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || fail "--version requires a release tag"
            release_version="$2"
            shift 2
            ;;
        --update)
            mode="update"
            shift
            ;;
        --uninstall)
            mode="uninstall"
            shift
            ;;
        --system)
            system_install=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

[ "$(uname -s)" = "Linux" ] || fail "only Linux is supported"

if "$system_install"; then
    install_dir="$SYSTEM_INSTALL_DIR"
else
    install_dir="$USER_INSTALL_DIR"
fi
binary_path="$install_dir/oom-tui"

run_privileged() {
    if "$system_install" && [ "$install_dir" = "/usr/local/bin" ] && [ "$(id -u)" -ne 0 ]; then
        command -v sudo >/dev/null 2>&1 || fail "--system requires root or sudo"
        sudo "$@"
    else
        "$@"
    fi
}

if [ "$mode" = "uninstall" ]; then
    if [ -e "$binary_path" ]; then
        run_privileged rm -f "$binary_path"
        printf 'Removed %s\n' "$binary_path"
    else
        printf 'oom-tui is not installed at %s\n' "$binary_path"
    fi
    exit 0
fi

case "$release_version" in
    latest)
        release_path="latest/download"
        ;;
    v[0-9]*|[0-9]*)
        release_version="v${release_version#v}"
        release_path="download/${release_version}"
        ;;
    *)
        fail "release tags must be 'latest' or start with a version number"
        ;;
esac

machine="${OOM_TUI_UNAME_MACHINE:-$(uname -m)}"
case "$machine" in
    x86_64|amd64)
        target="x86_64-unknown-linux-musl"
        ;;
    aarch64|arm64)
        target="aarch64-unknown-linux-musl"
        ;;
    *)
        fail "unsupported architecture: $machine"
        ;;
esac

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

archive="oom-tui-${target}.tar.gz"
checksum="${archive}.sha256"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT HUP INT TERM

download() {
    curl --fail --silent --show-error --location "$1" --output "$2"
}

printf 'Downloading oom-tui for %s...\n' "$target"
download "${RELEASE_BASE_URL}/${release_path}/${archive}" "${tmpdir}/${archive}" \
    || fail "could not download ${archive}"
download "${RELEASE_BASE_URL}/${release_path}/${checksum}" "${tmpdir}/${checksum}" \
    || fail "could not download ${checksum}"

(cd "$tmpdir" && sha256sum --check "$checksum") \
    || fail "checksum verification failed"

tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"
extracted_binary="$(find "$tmpdir" -type f -path '*/oom-tui' -print -quit)"
[ -n "$extracted_binary" ] || fail "release archive did not contain oom-tui"

run_privileged mkdir -p "$install_dir"
run_privileged install -m 0755 "$extracted_binary" "$binary_path"

if [ "$mode" = "update" ]; then
    printf 'Updated oom-tui at %s\n' "$binary_path"
else
    printf 'Installed oom-tui at %s\n' "$binary_path"
fi
