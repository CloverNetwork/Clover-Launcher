# Clover Launcher

A Minecraft: Java Edition launcher built with Tauri, Rust and Svelte.

- Vanilla, Fabric, Quilt, Forge, NeoForge, Legacy Fabric, Ornithe and Babric
- Import any launcher profile JSON for other loaders
- Multiple Microsoft accounts
- Skins and capes

## Building

You need Rust, Node and pnpm.

```
pnpm install
pnpm tauri dev
```

`pnpm tauri build` makes a release build.

## Notes

Sign-in uses the Azure app ID in `src-tauri/src/auth.rs`. It needs "Allow public client
flows" turned on in Azure, and Mojang has to approve it before Minecraft accepts its tokens.
Debug builds have an offline account for testing in the meantime.

Java is picked per version. If the right one isn't installed, Clover downloads Mojang's
runtime into its data folder. On Apple silicon, old versions that need Java 8 run under Rosetta.
You can set one Java for everything in Settings.
