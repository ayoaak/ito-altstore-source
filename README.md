# Ito MangaDex source

This repository contains an Ito-native MangaDex plugin adapted from the behavior of the Aidoku Community `multi.mangadex` source. It is a clean Ito implementation and does not load Aidoku source bundles directly.

## Add to Ito

After the first successful GitHub Actions build, add this repository URL in Ito:

```text
https://raw.githubusercontent.com/ayoaak/ito-altstore-source/main/repo/index.json
```

The plugin currently provides:

- Japanese-origin manga discovery
- Latest updates and title search
- Manga details, covers, authors, artists, tags, and status
- English-translated chapter lists
- MangaDex At-Home page loading

## Development

The Ito source is in [`sources/mangadex`](sources/mangadex). Pushes that change the source or build workflow automatically:

1. Build the plugin for `wasm32-unknown-unknown`
2. Package and verify the `.ito` file with the official `ito-pkg` tool
3. Regenerate the static Ito repository under `repo/`
4. Commit the generated repository back to `main`

You can also run the **Build Ito MangaDex repository** workflow manually.

## Upstream

- [Aidoku Community MangaDex source](https://github.com/Aidoku-Community/sources/tree/main/sources/multi.mangadex)
- [Ito plugin SDK](https://github.com/itoapp/ito-rs)
- [Ito packaging tool](https://github.com/itoapp/ito-pkg)
- [MangaDex API](https://api.mangadex.org/docs/)

This project is not affiliated with MangaDex, Aidoku, or Ito.

