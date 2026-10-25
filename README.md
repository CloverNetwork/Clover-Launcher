# Clover Launcher

A Minecraft: Java Edition launcher built with Tauri, Rust and Svelte.

- Vanilla, Fabric, Quilt, Forge, NeoForge, Legacy Fabric, Ornithe and Babric
- Import any launcher profile JSON for other loaders
- Mods from Modrinth and CurseForge, with dependencies and updates (CurseForge needs an API key in Settings)
- Modpacks from Modrinth and CurseForge, or a `.mrpack` / CurseForge zip from disk
- Multiple Microsoft accounts
- Skins and capes

## Building

You need Rust, Node and pnpm.

```
pnpm install
pnpm tauri dev
```

`pnpm tauri build` makes a release build.

The launcher itself lives in `crates/clover-core`, a plain Rust library with no UI.
`src-tauri` and `src` are the app on top of it. `cargo test --workspace` runs the tests.

## Notes

Sign-in uses the Azure app ID in `crates/clover-core/src/auth.rs`. It needs "Allow public client
flows" turned on in Azure, and Mojang has to approve it before Minecraft accepts its tokens.
Debug builds have an offline account for testing in the meantime.

Java is picked per version. If the right one isn't installed, Clover downloads Mojang's
runtime into its data folder. On Apple silicon, old versions that need Java 8 run under Rosetta.
You can set one Java for everything in Settings.

CurseForge mods need an API key approved for third-party apps
([apply here](https://forms.monday.com/forms/dce5ccb7afda9a1c21dab1a1aa1d84eb?r=use1)). Paste it in
Settings, or build it in with `CURSEFORGE_API_KEY='...' pnpm tauri build`. Use single quotes,
because keys contain `$`.
