#!/usr/bin/env bash
# Build the flatpak locally: bundle the deb, stage its contents and run
# flatpak-builder against flatpak/dev.igdm.client.yml.
#
# The flatpak consumes an extracted .deb (see the manifest header) so the
# sandbox build stays offline and identical to the deb build.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

target="${1:-}"
bundle_dir="target${target:+/$target}/release/bundle/deb"

echo "==> building the deb"
(cd crates/igdm && tauri build --bundles deb ${target:+--target "$target"})

echo "==> staging $bundle_dir/*.deb"
rm -rf flatpak/stage
mkdir -p flatpak/stage
# shellcheck disable=SC2086
dpkg-deb -x $bundle_dir/*.deb flatpak/stage

echo "==> flatpak-builder"
rm -rf flatpak/build-dir flatpak/repo
flatpak-builder --user --force-clean --disable-rofiles-fuse \
  --repo=flatpak/repo flatpak/build-dir flatpak/dev.igdm.client.yml

mkdir -p dist
flatpak build-bundle flatpak/repo dist/igdm.flatpak dev.igdm.client stable
echo "==> dist/igdm.flatpak"
