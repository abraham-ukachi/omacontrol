# Contributing to OmaControl 🤝

Thanks for helping! OmaControl is small on purpose: one remote, many devices, Omarchy's look.


## Getting started 🚀

```sh
git clone https://github.com/abraham-ukachi/omacontrol.git && cd omacontrol
cargo test -p omacontrol-core     # protocols, IR, themes (no TV needed)
cargo run -p omacontrol           # the UI on your desktop (handy for design work)
```

Android (needs the NDK and `cargo install cargo-apk`):

```sh
cargo apk run --lib -p omacontrol   # build, install and start on a plugged-in phone
```


## Project layout 🗂️

| No. | Path | What lives there |
|:----|:-----|:-----------------|
| 1 | `crates/omacontrol-core` | Discovery, protocols, IR, themes, storage. No UI, fully tested |
| 2 | `app/ui/*.slint` | The screens, in Slint |
| 3 | `app/src` | The glue: UI callbacks, the worker thread, Android entry point & IR blaster |
| 4 | `docs/design` | The reference mockups |


## Design rules 🎨

1. Follow **Omacalc**: page color + ink color, everything else is a mix of the two (5% keys, 13% borders, 16% operators & dividers, 50% muted). The primary key is inverted ink.
2. iA Writer Mono everywhere. No icons with fills, 1.6 px rounded strokes only.
3. The accent color is a whisper: the "connected" dot and the selected theme, nothing else.


## Git commit messages ✍️

We use [gitmoji](https://gitmoji.dev) + a short sentence in the imperative mood:

| Emoji | Code | When |
|:------|:-----|:-----|
| ✨ | `:sparkles:` | a new feature |
| 🐛 | `:bug:` | a bug fix |
| 💄 | `:lipstick:` | UI & style |
| 📝 | `:memo:` | docs |
| 🔧 | `:wrench:` | config & tooling |
| ✅ | `:white_check_mark:` | tests |
| 👷 | `:construction_worker:` | CI |
| 🔖 | `:bookmark:` | a release (`Release 0.2.0`) |

Example: `:sparkles: Add the Android TV pairing PIN screen`

## Releases 🔖

Bump `[workspace.package] version` in `Cargo.toml` and `Cargo.lock`, add the CHANGELOG entry, commit (`:bookmark: Release X.Y.Z-beta`), then push the `vX.Y.Z-beta` tag right away. The "Android, Linux and iOS" workflow checks the tag against the version and **every release ships all three builds**: the Android APK, the Linux tarball (`omacontrol-linux-x86_64.tar.gz`) and the iOS app (`omacontrol-vX.Y.Z-unsigned.ipa`, plus a signed `.ipa` when the `IOS_CERT_P12_BASE64`, `IOS_CERT_PASSWORD` and `IOS_PROVISION_PROFILE_BASE64` secrets are set). A release missing one of them isn't done.

iOS builds need macOS with Xcode and `xcodegen` (`brew install xcodegen`): `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, then `ios/generate.sh` and open `ios/OmaControl.xcodeproj`. Code that only exists on a phone goes behind `#[cfg(target_os = "android")]` or `#[cfg(target_os = "ios")]`.
