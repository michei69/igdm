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
- **misc:** media preview overlay with download, per-message/thread "copy raw data", configurable reaction emojis (separate settings window). seen/typing published over MQTT when connected, HTTP fallback otherwise.

## setup

get the sloppy [bun](https://bun.sh), rust, and tauri's linux deps (webkit2gtk-4.1, gtk3, libsoup3, pkg-config)

```bash
bun install                    # root deps: tauri CLI
bun run frontend:install       # frontend deps (bun install --cwd crates/igdm/ui)
bun run dev                    # dev: Vite HMR + hot rust reload
bun run build                  # release -> target/release/bundle/<all host bundles>
bun run build:linux            # deb + rpm + appimage
bun run build:flatpak          # flatpak (needs flatpak-builder; stages the deb, see flatpak/)
bun run run                    # cargo run -p igdm (embeds built frontend)
```

`cargo run -p igdm` works straight from the root too — `crates/igdm/build.rs` builds the frontend automatically when `ui/dist` is missing (bypass with `IGDM_SKIP_FRONTEND=1`, e.g. headless CI). but `cargo run` embeds whatever's in `ui/dist` - use `bun run dev` for live frontend changes. sessions live in `~/.igdm/sessions/` (override with `IGDM_SESSIONS`)

## builds

`.github/workflows/build.yml` builds every supported desktop target on a tag push (`v*`), a push to `main`, or manually, and attaches the installers to a GitHub Release on tags:

| target | artifact |
| --- | --- |
| linux x86_64 | `.deb`, `.rpm`, `.AppImage` |
| linux aarch64 | `.deb`, `.rpm`, `.AppImage` |
| windows x86_64 | NSIS `.exe` |
| windows aarch64 | NSIS `.exe` |
| macOS aarch64 | `.app`, `.dmg` |
| flatpak x86_64 / aarch64 | `igdm.flatpak` |

macOS builds are ad-hoc signed (`APPLE_SIGNING_IDENTITY: "-"`) because the repo has no Apple certificate, so Gatekeeper needs a right-click → Open (or `xattr -dr com.apple.quarantine`) the first time. set `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD`/`APPLE_SIGNING_IDENTITY` (and `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` for notarization) as repo secrets to get a signed, notarized build instead.

the flatpak job reuses the linux `.deb` (extracted with `dpkg-deb`), so `flatpak/dev.igdm.client.yml` never needs network access to crates.io/npm.

## architecture

- `crates/instagrapi/` - instagram private api client (rust port)
- `crates/igdm/src/` - tauri backend
- `crates/igdm/tauri.conf.json` — window config, bundle, webview UA
- `crates/igdm/ui/` — react frontend
- `flatpak/` - flatpak manifest + appstream metainfo (packages the deb produced by `crates/igdm`, see `scripts/build-flatpak.sh`)
- `.github/workflows/build.yml` - cross-platform build + release workflow

## is this slop

fuck yes this is

i have better stuff to spend my time on than this

## notes

- **attachments are picked by the backend:** the composer calls the `pick_media` command, which opens the native dialog in rust and remembers the chosen path; `send_photo`/`send_video` only accept paths from that set, so a compromised webview cannot ask the backend to upload `~/.ssh/id_rsa`.
- **voice notes on linux:** webkitgtk denies `getUserMedia` unless the app handles its permission request, which neither wry nor tauri does — `enable_media_capture` in `crates/igdm/src/lib.rs` enables the media stream and grants requests for the app's own origin.
- **appimage builds set `NO_STRIP=1`:** linuxdeploy bundles a `strip` that cannot read the `.relr.dyn` section modern linkers emit (Arch, Fedora, Ubuntu 24.04+), and it aborts when strip fails. the system libraries it would strip are already stripped by the distro. plain `bun run build` (and the workflow's other platforms) don't need it; the workflow suppresses it for the linux job only.
- **no more vendored `tao`:** tao 0.36.0 removed the client-side decorations tao 0.35.x forced on wayland windows (broken titlebars on KDE - see tauri-apps/tao#1046, tauri-apps/tauri#12955, tauri-apps/tauri#12685). tauri 2.11.x still pinned `tao = "0.35"`, so this repo used to vendor a patched 0.35.3 tree; tauri 2.12 ships `tao = "0.37"`, which has the fix upstream, so the vendor directory and the `[patch.crates-io]` entry are gone.
- instagram's private realtime api is undocumented and can change whenever it wants; MQTT support is inherited from instagrapi and marked experimental there.
- use responsibly: automated access violates IG's ToS and aggressive use gets accounts challenged or banned. if u get banned tho its not my fault :3
