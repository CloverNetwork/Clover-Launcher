<img src="src-tauri/icons/128x128.png" width="64" alt="">

# Clover Launcher

A Minecraft: Java Edition launcher built with Tauri 2, Rust and SvelteKit (Svelte 5).

```bash
pnpm install
pnpm tauri dev
```

## Before you can play

1. **Microsoft sign-in.** Register a public client app (personal Microsoft accounts) in Azure,
   put its ID in `CLIENT_ID` in `src-tauri/src/auth.rs`, and request Minecraft API access at
   <https://aka.ms/mce-reviewappid>. Approval can take a while, so start early.
2. **Java.** Install the Java version the game asks for (21 for current releases) or point
   Settings (⌘,) at it. Bundling Mojang's runtimes is a TODO.

## Layout

| Path | What it does |
| --- | --- |
| `src-tauri/src/manifest.rs` | Version manifest and launcher profile types, OS rules, Maven paths |
| `src-tauri/src/profile.rs` | Resolves a profile's `inheritsFrom` chain into one launchable version |
| `src-tauri/src/loaders.rs` | Mod loader sources: meta servers, Forge/NeoForge installers, imported profiles |
| `src-tauri/src/install.rs` | Prepares the loader, then downloads client, libraries, natives, assets, log config |
| `src-tauri/src/download.rs` | Parallel, SHA-1 verified downloads; valid files are skipped |
| `src-tauri/src/launch.rs` | Classpath, JVM and game arguments, legacy natives extraction |
| `src-tauri/src/game.rs` | Spawns Java, streams the log (log4j XML turned into text), stops the game |
| `src-tauri/src/smoke.rs` | End-to-end tests against real loader servers (ignored by default) |
| `src-tauri/src/auth.rs` | Device code → Xbox Live → XSTS → Minecraft; accounts, refresh tokens in the OS keychain, session cache |
| `src-tauri/src/skins.rs` | Mojang skin and cape API, local skin library, texture cache |
| `src-tauri/src/menu.rs` | Menu bar; every window command is also a menu command |
| `src/lib/launcher.svelte.ts` | App state and actions |
| `src/lib/components/` | Sidebar, detail view, sheets, game log, generated block icons |
| `src/routes/settings/` | The settings window |

Data lives in `~/Library/Application Support/dev.blackdevlabs.clover` (macOS).

## Mod loaders

Mod loaders are optional: pick one in New Instance, or leave it at None. Clover doesn't hard-code
loaders; it supports three kinds, so loaders it has never heard of still work.

| Kind | How it works | Built in |
| --- | --- | --- |
| Meta server | Any server with Fabric's meta API: `/versions/<path>/<game>` lists versions, `.../<loader>/profile/json` returns a profile | Fabric, Quilt, Legacy Fabric, Ornithe, Ornithe (Quilt Loader), Babric |
| Installer | Runs the loader's installer with `--installClient` into Clover's folder | Forge, NeoForge |
| Imported profile | Any launcher profile JSON (`inheritsFrom` a vanilla version, or standalone) | — |

Add more meta servers in Settings › Extra Mod Loaders (name, address including the API version,
and the loader path, usually `loader`). For anything else, run the loader's own installer for the
official launcher, then choose Import Launcher Profile and pick the JSON it wrote.

Every route ends as a profile in `versions/<id>/<id>.json`. `profile.rs` merges it with its
parents the way the official launcher does. Scalar fields come from the nearest profile,
arguments concatenate, and a loader's library replaces the vanilla library with the same group
and artifact. The client jar is copied to `versions/<id>/<id>.jar` because Forge and NeoForge
ignore a jar by that name when they build their module layer.

Tested end to end (`cargo test smoke -- --ignored --test-threads 1`). Every built-in loader
installs, and these boot to the title screen on an Apple silicon Mac with Java 26: vanilla 1.21.4
with Fabric, Quilt, NeoForge and Forge, and Legacy Fabric 1.8.9. Two old versions need a
different Java: Babric (Beta 1.7.3) needs Java 8, and vanilla 1.8.9 under Ornithe ships
Intel-only LWJGL natives.

## Skins and capes

Game › Account › Skins & Capes… (⇧⌘K) opens the wardrobe:

- **Preview:** a pixel-exact front and back view of any skin, both layers, with the selected cape
  on the back. It handles 64×64 skins and the old 64×32 layout.
- **Library:** skin files shared by every account, stored in `skins/`. Add PNGs, right-click to
  remove one, or save the skin an account is wearing so you can switch back later.
- **Capes:** the capes the account owns, plus No Cape.

Choices are previewed and only sent to Mojang on Apply, because skin uploads are rate-limited.
Offline accounts can build a library but can't wear skins. Minecraft access tokens are cached in
memory for 23 hours, so wardrobe actions and launches skip the four-step sign-in.

## Branding

`branding/clover-logo.svg` is the original mark. `branding/app-icon.svg` places it on the macOS
icon grid (824 pt rounded square on a 1024 canvas) in clover green. Regenerate every icon size
after editing it:

```bash
pnpm tauri icon branding/app-icon.svg
```

Inside the app the mark appears only on the welcome screen (`CloverLogo.svelte`), in one colour
so it follows light and dark appearance.

## Design notes

Built against Apple's Human Interface Guidelines for macOS:

- System font, semantic color tokens for light, dark and increased contrast, and the system
  accent color. There's no in-app appearance switch.
- Overlay title bar with sidebar vibrancy on macOS; opaque elsewhere and under Reduce Transparency.
- The sidebar shows one level, can be hidden (⌃⌘S), and collapses in narrow windows. The account
  button sits in the toolbar, not at the bottom of the sidebar.
- Errors appear inline in the detail view. Only the irreversible Delete uses an alert.
- Settings opens in its own window (⌘,) and applies changes immediately.
- Signature element: each instance gets a pixel block face generated from its ID
  (`BlockIcon.svelte`). Everything else stays quiet, and the logo isn't repeated through the app.

## Next steps

- Bundle Java runtimes from Mojang's `java-runtime` manifest, picked by each profile's `javaVersion`
- Swap in arm64 LWJGL 2 natives on Apple silicon for pre-1.13 versions
- Legacy asset layouts (`pre-1.6`, `legacy`) for very old versions
- Modrinth search and install
- Signed updates via the Tauri updater
