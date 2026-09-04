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
bun run frontend:install       # frontend deps (bun --cwd crates/igdm/ui install)
bun run dev                    # dev: Vite HMR + hot rust reload
bun run build                  # release -> target/release/bundle/{deb,rpm}
bun run run                    # cargo run -p igdm (embeds built frontend)
```

`cargo run -p igdm` works straight from the root too — `crates/igdm/build.rs` builds the frontend automatically when `ui/dist` is missing (bypass with `IGDM_SKIP_FRONTEND=1`, e.g. headless CI). but `cargo run` embeds whatever's in `ui/dist` - use `bun run dev` for live frontend changes. sessions live in `~/.igdm/sessions/` (override with `IGDM_SESSIONS`)

## architecture

- `crates/instagrapi/` - instagram private api client (rust port)
- `crates/igdm/src/` - tauri backend
- `crates/igdm/tauri.conf.json` — window config, bundle, webview UA
- `crates/igdm/ui/` — react frontend

## is this slop

fuck yes this is

i have better stuff to spend my time on than this

## notes

- **vendored `tao` patch:** tao 0.36.0 removed the client-side decorations tao 0.35.x forced on wayland windows (broken titlebars on KDE - see tauri-apps/tao#1046, tauri-apps/tauri#12955, tauri-apps/tauri#12685). tauri 2.11.x still pins `tao = "0.35"`, so the 0.35.3 tree with the upstream 0.36.0 `window.rs` change is vendored in `vendor/tao` and wired via `[patch.crates-io]` in `Cargo.toml`. remove it once tauri >= 2.12 ships.
- instagram's private realtime api is undocumented and can change whenever it wants; MQTT support is inherited from instagrapi and marked experimental there.
- use responsibly: automated access violates IG's ToS and aggressive use gets accounts challenged or banned. if u get banned tho its not my fault :3
