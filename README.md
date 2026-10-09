# IG Direct

![](https://raw.githubusercontent.com/michei69/disclaimers/refs/heads/main/ai/x2.png)
![](https://raw.githubusercontent.com/michei69/disclaimers/refs/heads/main/no-maintenance/x2.png)

a desktop client for [instagram](https://instagram.com) DMs cuz i fucking hate the web slop meta has

built with tauri 2, react 19 + react router, tailwind css v4, daisyUI (typescript), rust backend

## why was this built?

big tech cant make a functioning web app and i unfortunately need to keep in contact with friends on this fuckass platform. so we got [instagrapi](https://github.com/subzeroid/instagrapi) ported here: `crates/instagrapi` is a from-scratch `#![forbid(unsafe_code)]` rust port of just the surface this app touches

and also cuz there isnt any other 3rd party desktop client for this

## feats

- **login:** username + password (2FA + SMS/email) or `sessionid` cookie. sessions cached in `~/.igdm/sessions/` for one-click resume - same JSON format instagrapi uses, so old sessions carry over.
- **live messaging:** MQTToT realtime connection with auto-reconnect, idle keepalive pings, per-reconnect handler re-registration. connection status shown in the sidebar.
- **inbox:** threads, unread markers, avatars, search (existing chats + new 1:1s).
- **message pane:** gradient bubbles, day separators, load-older pagination (scroll-up or banner), typing indicators, seen receipts, reactions, media/voice/GIF/post/sticker payloads.
- **send:** text (with reply quoting), photos, videos; message-request approval.
- **tray:** always-on tray icon - left click shows/hides the window, right click opens the Open/Exit menu. closing a window hides it instead of quitting, so the app keeps receiving messages (and notifying) while it is away.
- **misc:** media preview overlay with download, per-message/thread "copy raw data", configurable reaction emojis (separate settings window). seen/typing published over MQTT when connected, HTTP fallback otherwise.

## setup

get the sloppy [bun](https://bun.sh), rust, and tauri's linux deps (webkit2gtk-4.1, gtk3, libsoup3, pkg-config)

```bash
bun install                    # root deps: tauri CLI
bun run frontend:install       # frontend deps (bun install --cwd crates/igdm/ui)
bun run dev                    # dev: Vite HMR + hot rust reload
bun run build                  # release -> target/release/bundle/<all host bundles>
bun run build:linux            # deb + rpm + appimage -> dist/ (adds the tarballs)
bun run build:flatpak          # flatpak (needs flatpak-builder)
bun run build:snap             # snap (needs snapcraft)
bun run run                    # cargo run -p igdm (embeds built frontend)
```

`cargo run -p igdm` works straight from the root too — `crates/igdm/build.rs` builds the frontend automatically when `ui/dist` is missing (bypass with `IGDM_SKIP_FRONTEND=1`, e.g. headless CI). but `cargo run` embeds whatever's in `ui/dist` - use `bun run dev` for live frontend changes. sessions live in `~/.igdm/sessions/` (override with `IGDM_SESSIONS`; the snap sets this to its own `$SNAP_USER_COMMON` because `home` does not cover dotfiles under strict confinement)

## builds

`scripts/package-linux.sh` is the one entry point for linux: it runs after `tauri build` and normalises everything into `dist/` under a single `igdm-<version>-linux-<arch>.<ext>` naming scheme. tauri itself only knows `deb`, `rpm` and `appimage`, so the rest is assembled here.

| installer | how | notes |
| --- | --- | --- |
| `.AppImage` | tauri (`linuxdeploy`) | one file, no install, bundles webkit; needs FUSE (or `--appimage-extract-and-run`) |
| `.AppDir.tar.gz` | this repo | the AppImage's AppDir, for machines without FUSE |
| `.deb` | tauri | debian/ubuntu/mint/pop; `Depends: libwebkit2gtk-4.1-0, libgtk-3-0` |
| `.rpm` | tauri | fedora/rhel/opensuse; needs `rpmbuild` |
| `.tar.gz` | this repo | portable prefix (`bin/`, `share/`): run in place or unpack into `~/.local`; no root, no package manager |
| `.snap` | `snap/snapcraft.yaml` | ubuntu (and any distro with snapd); strict confinement, bundles webkit + ffmpeg |
| `.flatpak` | `flatpak/dev.igdm.client.yml` | distro-agnostic sandbox; bundles the GNOME runtime |

the portable tarball is the `.deb`'s file tree with `usr/` stripped (`bin/igdm` + `share/`), and it is what the flatpak consumes; the snap consumes the `.deb` directly. all three therefore ship the exact same binary and none of them need network access to crates.io or npm.

note the `.AppDir.tar.gz` is ~126 MB: it is the fully self-contained bundle (webkit and friends included), the same content the AppImage wraps. skip it with `IGDM_SKIP_APPDIR=1` if you only want the AppImage.

for the other desktops:

| target | installer |
| --- | --- |
| windows x86_64 / arm64 | NSIS `.exe` (per-user, no admin) |
| macOS universal | `.dmg` + `.app.tar.gz` (one bundle for intel + apple silicon) |

## ci

`.github/workflows/build.yml` is the reusable build: it runs on pushes to `main`, pull requests and manual dispatches, and builds every installer above on native runners (linux x64 + arm64, windows x64 + arm64, macOS universal). `.github/workflows/release.yml` calls it on a `v*` tag, verifies the tag matches the workspace version, writes `SHA256SUMS`, and attaches everything to a GitHub Release:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

nothing is uploaded to the snap store or to Flathub; both are separate, opt-in steps (`snapcraft upload`, or a Flathub repo pointing at `flatpak/dev.igdm.client.yml`).

macOS builds are ad-hoc signed (`APPLE_SIGNING_IDENTITY: "-"`) because the repo has no Apple certificate, so Gatekeeper needs a right-click → Open (or `xattr -dr com.apple.quarantine`) the first time. set `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD`/`APPLE_SIGNING_IDENTITY` (and `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` for notarization) as repo secrets to get a signed, notarized build instead; the workflow only exports the ones that are actually set, so forks and PRs still build.

## architecture

- `crates/instagrapi/` - instagram private api client (rust port)
- `crates/igdm/src/` - tauri backend
- `crates/igdm/tauri.conf.json` — window config, bundle, webview UA
- `crates/igdm/ui/` — react frontend
- `scripts/package-linux.sh` - assembles `dist/` (tarballs + normalised names)
- `scripts/build-flatpak.sh`, `scripts/build-snap.sh` - local flatpak/snap builds
- `flatpak/` - flatpak manifest + appstream metainfo
- `snapcraft.yaml` (repo root: snapcraft resolves part sources from the directory it runs in) + `snap/gui/` assets
- `.github/workflows/` - reusable build + tag release workflows

## is this slop

fuck yes this is

i have better stuff to spend my time on than this

## notes

- **attachments are picked by the backend:** the composer calls the `pick_media` command, which opens the native dialog in rust and remembers the chosen path; `send_photo`/`send_video` only accept paths from that set, so a compromised webview cannot ask the backend to upload `~/.ssh/id_rsa`.
- **voice notes on linux:** webkitgtk denies `getUserMedia` unless the app handles its permission request, which neither wry nor tauri does — `enable_media_capture` in `crates/igdm/src/lib.rs` enables the media stream and grants requests for the app's own origin.
- **appimage builds set `NO_STRIP=1`:** linuxdeploy bundles a `strip` that cannot read the `.relr.dyn` section modern linkers emit (Arch, Fedora, Ubuntu 24.04+), and it aborts when strip fails. the system libraries it would strip are already stripped by the distro. plain `bun run build` (and the workflow's other platforms) don't need it; the workflow suppresses it for the linux job only.
- **no more vendored `tao`:** tao 0.36.0 removed the client-side decorations tao 0.35.x forced on wayland windows (broken titlebars on KDE - see tauri-apps/tao#1046, tauri-apps/tauri#12955, tauri-apps/tauri#12685). tauri 2.11.x still pinned `tao = "0.35"`, so this repo used to vendor a patched 0.35.3 tree; tauri 2.12 ships `tao = "0.37"`, which has the fix upstream, so the vendor directory and the `[patch.crates-io]` entry are gone.
- **`icons/icon.png` must stay first in `bundle.icon`:** the linux packages get every png in that list (the bundler installs each one under `hicolor/<w>x<h>/apps/igdm.png`), but the *runtime* window/taskbar icon is only the **first** png in the list — tauri embeds that single file as `default_window_icon`. it used to be `icons/32x32.png`, which is why the taskbar icon was a blurry upscale; it is now the 512×512 `icons/icon.png`. windows uses `icon.ico` and macOS uses `icon.icns` (both already carry the full size range, 16–256 and 32–1024 respectively).
- instagram's private realtime api is undocumented and can change whenever it wants; MQTT support is inherited from instagrapi and marked experimental there.
- use responsibly: automated access violates IG's ToS and aggressive use gets accounts challenged or banned. if u get banned tho its not my fault :3
