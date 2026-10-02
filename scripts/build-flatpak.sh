#!/usr/bin/env bash
# Build the flatpak locally: bundle the .deb, assemble the portable tarball,
# stage its contents and run flatpak-builder against
# flatpak/dev.igdm.client.yml.
#
# The flatpak consumes that tarball (see the manifest header) so the sandbox
# build stays offline and identical to the deb build.
#
#   scripts/build-flatpak.sh [target-triple]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

target="${1:-}"

echo "==> building the deb + portable tarball"
(cd crates/igdm && tauri build --bundles deb ${target:+--target "$target"})
scripts/package-linux.sh ${target:+"$target"}

version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
case "${target:-$(uname -m)}" in
  *aarch64* | *arm64*) arrch64 ;;
  *x86_64* | *amd64*) arch=x86_64 ;;
  *)
    echo "unsupported architecture: ${target:-$(uname -m)}" >&2
    exit 1
    ;;
esac

tarball="dist/igdm-$version-linux-$arch.tar.gz"

echo "==> staging $tarball"
rm -rf flatpak/stage
mkdir -p flatpak/stage
tar -xzf "$tarball" --strip-components=1 -C flatpak/stage

echo "==> flatpak-builder"
rm -rf flatpak/build-dir flatpak/repo
flatpak-builder --user --force-clean --disable-rofiles-fuse \
  --repo=flatpak/repo flatpak/build-dir flatpak/dev.igdm.client.yml

out="dist/igdm-$version-linux-$arch.flatpak"
flatpak build-bundle flatpak/repo "$out" dev.igdm.client stable
echo "==> $out"
