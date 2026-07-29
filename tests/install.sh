#!/bin/sh

set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT HUP INT TERM

fail() {
    printf '%s\n' "install test: $*" >&2
    exit 1
}

make_release() {
    target="$1"
    version="$2"
    message="$3"
    archive="oom-tui-${target}.tar.gz"
    package_dir="${tmpdir}/package/oom-tui-${version}-${target}"
    release_dir="${tmpdir}/releases/download/${version}"

    mkdir -p "$package_dir" "$release_dir" "${tmpdir}/releases/latest/download"
    printf '#!/bin/sh\nprintf "%s\\n"\n' "$message" > "${package_dir}/oom-tui"
    chmod +x "${package_dir}/oom-tui"
    tar -C "${tmpdir}/package" -czf "${release_dir}/${archive}" "$(basename "$package_dir")"
    sha256sum "${release_dir}/${archive}" > "${release_dir}/${archive}.sha256"
    cp "${release_dir}/${archive}" "${tmpdir}/releases/latest/download/${archive}"
    cp "${release_dir}/${archive}.sha256" "${tmpdir}/releases/latest/download/${archive}.sha256"
}

run_installer() {
    OOM_TUI_RELEASE_BASE_URL="file://${tmpdir}/releases" \
    OOM_TUI_INSTALL_DIR="${tmpdir}/user-bin" \
    OOM_TUI_SYSTEM_DIR="${tmpdir}/system-bin" \
    "$root/install.sh" "$@"
}

make_release x86_64-unknown-linux-musl v0.1.0 x86
make_release aarch64-unknown-linux-musl v0.1.0 arm

run_installer
[ "$("${tmpdir}/user-bin/oom-tui")" = "x86" ] || fail "default install selected the wrong archive"

run_installer --update
[ "$("${tmpdir}/user-bin/oom-tui")" = "x86" ] || fail "update did not preserve the selected target"

run_installer --uninstall
[ ! -e "${tmpdir}/user-bin/oom-tui" ] || fail "uninstall left the user binary behind"

OOM_TUI_UNAME_MACHINE=arm64 run_installer --version 0.1.0
[ "$("${tmpdir}/user-bin/oom-tui")" = "arm" ] || fail "arm64 did not select the arm archive"

run_installer --system
[ "$("${tmpdir}/system-bin/oom-tui")" = "x86" ] || fail "system install used the wrong destination"
run_installer --system --uninstall
[ ! -e "${tmpdir}/system-bin/oom-tui" ] || fail "system uninstall left the binary behind"

if OOM_TUI_UNAME_MACHINE=riscv64 run_installer >/dev/null 2>&1; then
    fail "unsupported architecture unexpectedly succeeded"
fi

checksum="${tmpdir}/releases/latest/download/oom-tui-x86_64-unknown-linux-musl.tar.gz.sha256"
printf '%064d  %s\n' 0 "oom-tui-x86_64-unknown-linux-musl.tar.gz" > "$checksum"
if run_installer >/dev/null 2>&1; then
    fail "invalid checksum unexpectedly succeeded"
fi

printf '%s\n' "installer tests passed"
