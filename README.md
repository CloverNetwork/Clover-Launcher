# Clover Launcher

A Minecraft: Java Edition launcher built with Tauri, Rust and Svelte.

- Vanilla, Fabric, Quilt, Forge, NeoForge, Legacy Fabric, Ornithe and Babric
- Import any launcher profile JSON for other loaders
- Mods from Modrinth and CurseForge, with dependencies and updates (CurseForge needs an API key in Settings)
- Resource packs, shader packs and data packs
- Modpacks from Modrinth and CurseForge, or a `.mrpack` / CurseForge zip from disk; export as `.mrpack`
- Worlds with backups, screenshots, and a server list with status
- Join a server or open a world straight from the launcher
- Crash help that spots missing mods, the wrong Java and memory problems
- Discord status while playing
- One-click performance mods (Sodium, Lithium, FerriteCore and more)
- English and German
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

## Releasing

Pushing a version tag builds Clover for macOS, Windows and Linux and attaches the installers
to a draft GitHub release:

```
git tag v0.2.0
git push origin v0.2.0
```

Bump the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json` first.
Installed copies update themselves from the latest published release, so publish the draft
when it's ready.

The repository needs two secrets, from `pnpm tauri signer generate`:
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Updates signed with any
other key are refused. macOS builds are unsigned until Apple Developer ID secrets are added
(see the workflow file).

## Notes

Sign-in uses the Azure app ID in `crates/clover-core/src/auth.rs`. It needs "Allow public client
flows" turned on in Azure, and Mojang has to approve it before Minecraft accepts its tokens.
Debug builds have an offline account for testing in the meantime.

Java is picked per version. If the right one isn't installed, Clover downloads Mojang's
runtime into its data folder. On Apple silicon, old versions that need Java 8 run under Rosetta.
You can set one Java for everything in Settings.

Right-click an instance to add it to your Applications folder, app launcher or desktop, or to
copy a command that starts it, for example a Hyprland keybind:

```
bind = SUPER, M, exec, clover-launcher --launch <instance-id>
```

If Clover is already open, the running app starts the instance.

CurseForge mods need an API key approved for third-party apps
([apply here](https://forms.monday.com/forms/dce5ccb7afda9a1c21dab1a1aa1d84eb?r=use1)). Paste it in
Settings, or build it in with `CURSEFORGE_API_KEY='...' pnpm tauri build`. Use single quotes,
because keys contain `$`.

The Discord status uses the Clover Launcher application on Discord, with the player's head from
render.crafty.gg as the small image. Builds can use another application with `DISCORD_APP_ID=...`.
