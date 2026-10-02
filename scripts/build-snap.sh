#!/usr/bin/env bash
# Build the snap locally: bundle the deb, stage its contents into snap/stage/
# and run snapcraft against snapcraft.yaml (which lives at the repo root because
# snapcraft resolves part sources relative to where it is run).
#
# The snap consumes an extracted .deb (see the manifest header) so the
# snapcraft build needs no crates.io/npm access and ships the same binary as
# the deb and the flatpak.
#
#   scripts/build-snap.sh [target-triple]
#
# Needs snapcraft (`snap install snapcraft --classic`), which this repo's CI
# provides; it is not installed by default.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

target="${1:-}"
bundle_dir="target${target:+/$target}/release/bundle/deb"

version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
case "${target:-$(uname -m)}" in
  *aarch64* | *arm64*) arch=aarch64 ;;
  *x86_64* | *amd64*) arch=x86_64 ;;
  *)
    echo "unsupported architecture: ${target:-$(uname -m)}" >&2
    exit 1
    ;;
esac

echo "==> building the deb"
(cd crates/igdm && tauri build --bundles deb ${target:+--target "$target"})

echo "==> staging $bundle_dir/*.deb"
rm -rf snap/stage
mkdir -p snap/stage
# shellcheck disable=SC2086
dpkg-deb -x $bundle_dir/*.deb snap/stage

# Keep the manifest's version in step with the workspace, like CI does.
sed -i "s/^version: .*/version: '$version'/" snapcraft.yaml

echo "==> snapcraft"
snapcraft --destructive-mode

mkdir -p dist
snap_arch="$([ "$arch" = aarch64 ] && echo arm64 || echo amd64)"
mv -f "igdm_${version}_${snap_arch}.snap" "dist/igdm-$version-linux-$arch.snap"
echo "==> dist/igdm-$version-linux-$arch.snap"
