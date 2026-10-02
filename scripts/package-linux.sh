#!/usr/bin/env bash
# Assemble the Linux artifacts tauri does not produce natively, and normalise
# the ones it does, so `dist/` ends up as one consistently named set:
#
#   igdm-<ver>-linux-<arch>.deb              (tauri)
#   igdm-<ver>-linux-<arch>.rpm              (tauri)
#   igdm-<ver>-linux-<arch>.AppImage         (tauri)
#   igdm-<ver>-linux-<arch>.tar.gz           portable tree, installs without root
#   igdm-<ver>-linux-<arch>.AppDir.tar.gz    the AppImage's AppDir, runs without FUSE
#
# Run it after `tauri build --bundles deb,rpm,appimage`:
#
#   scripts/package-linux.sh [target-triple]
#
# The tarball is the .deb's file tree rearranged as a prefix (`bin/`, `share/`
# instead of `usr/bin/`, `usr/share/`) so it can either be run in place or
# unpacked into `~/.local`. It links against the system webkit2gtk/gtk, exactly
# like the .deb; use the AppImage or the AppDir if you want them bundled.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

target="${1:-}"
bundle_dir="target${target:+/$target}/release/bundle"
dist="${DIST_DIR:-dist}"

version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
if [ -z "$version" ]; then
  echo "could not read the workspace version from Cargo.toml" >&2
  exit 1
fi

case "${target:-$(uname -m)}" in
  *aarch64* | *arm64*) arch=aarch64 ;;
  *x86_64* | *amd64*) arch=x86_64 ;;
  *)
    echo "unsupported architecture: ${target:-$(uname -m)}" >&2
    exit 1
    ;;
esac

name="igdm-$version-linux-$arch"
if [ ! -d "$bundle_dir" ]; then
  echo "no bundles at $bundle_dir — run \`tauri build\` first" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$dist"

deb="$(find "$bundle_dir/deb" -maxdepth 1 -name '*.deb' -print -quit 2>/dev/null || true)"
if [ -z "$deb" ]; then
  echo "no .deb in $bundle_dir/deb — the portable tarball is built from it" >&2
  exit 1
fi

echo "==> extracting $(basename "$deb")"
mkdir -p "$work/$name"
if command -v dpkg-deb >/dev/null 2>&1; then
  dpkg-deb -x "$deb" "$work/$name"
else
  # A .deb is an `ar` archive holding data.tar.{gz,xz,zst}; dpkg is not
  # installed everywhere (e.g. Arch, Fedora).
  (cd "$work" && ar x "$deb" && tar -xf data.tar.* -C "$name")
fi

# deb layout is `usr/bin`, `usr/share`; a prefix layout is `bin`, `share`.
if [ -d "$work/$name/usr" ]; then
  mv "$work/$name/usr"/* "$work/$name/"
  rmdir "$work/$name/usr"
fi

# Tauri names the desktop entry after `productName` ("IG Direct.desktop"), but
# the appstream metainfo it ships declares `dev.igdm.client.desktop`. Rename so
# the two agree; `Exec`/`Icon` already point at `igdm` and resolve either way.
desktop="$work/$name/share/applications/IG Direct.desktop"
if [ -f "$desktop" ]; then
  mv "$desktop" "$work/$name/share/applications/dev.igdm.client.desktop"
fi

echo "==> $dist/$name.tar.gz"
tar -czf "$dist/$name.tar.gz" -C "$work" "$name"

appdir="$(find "$bundle_dir/appimage" -maxdepth 1 -name '*.AppDir' -print -quit 2>/dev/null || true)"
if [ -n "$appdir" ] && [ "${IGDM_SKIP_APPDIR:-0}" != "1" ]; then
  echo "==> $dist/$name.AppDir.tar.gz"
  cp -a "$appdir" "$work/$name.AppDir"
  tar -czf "$dist/$name.AppDir.tar.gz" -C "$work" "$name.AppDir"
elif [ -n "$appdir" ]; then
  echo "note: IGDM_SKIP_APPDIR=1, skipping the AppDir tarball" >&2
else
  echo "note: no AppDir in $bundle_dir/appimage, skipping the AppDir tarball" >&2
fi

for f in "$bundle_dir"/deb/*.deb "$bundle_dir"/rpm/*.rpm "$bundle_dir"/appimage/*.AppImage; do
  [ -e "$f" ] || continue
  case "$f" in
    *.deb) out="$dist/$name.deb" ;;
    *.rpm) out="$dist/$name.rpm" ;;
    *.AppImage) out="$dist/$name.AppImage" ;;
    *) continue ;;
  esac
  cp -f "$f" "$out"
  echo "==> $out"
done

ls -1sh "$dist"
