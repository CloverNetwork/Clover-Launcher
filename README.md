# Launcher

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
| `src-tauri/src/manifest.rs` | Version manifest and version JSON types, OS rule evaluation |
| `src-tauri/src/install.rs` | Builds the download list: client, libraries, natives, assets, log config |
| `src-tauri/src/download.rs` | Parallel, SHA-1 verified downloads; valid files are skipped |
| `src-tauri/src/launch.rs` | Classpath, JVM and game arguments, legacy natives extraction |
| `src-tauri/src/game.rs` | Spawns Java, streams the log, stops the game |
| `src-tauri/src/auth.rs` | Device code → Xbox Live → XSTS → Minecraft; refresh token in the OS keychain |
| `src-tauri/src/menu.rs` | Menu bar; every window command is also a menu command |
| `src/lib/launcher.svelte.ts` | App state and actions |
| `src/lib/components/` | Sidebar, detail view, sheets, game log, generated block icons |
| `src/routes/settings/` | The settings window |

Data lives in `~/Library/Application Support/dev.blackdevlabs.mclauncher` (macOS).

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
  (`BlockIcon.svelte`). Everything else stays quiet.

## Next steps

- Bundle Java runtimes from Mojang's `java-runtime` manifest
- Fabric/Quilt (meta API) and NeoForge (installer) loaders
- Modrinth search and install
- Signed updates via the Tauri updater
