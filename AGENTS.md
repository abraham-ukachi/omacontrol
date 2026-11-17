# AGENTS.md — OmaControl 🤖

> Read this first if you are an AI agent (or a new human) picking up this repo.

**OmaControl** is a Rust + Slint remote control app (Android first) with Omarchy's look & feel.
The long-term goal: **one app for anything with an IR receiver or a network remote**. TVs first, then air conditioners (fixed and portable).

## Golden rules

1. 🦀 **Core first.** Protocol, IR and theme logic lives in `crates/omacontrol-core` with unit tests. The app crate only wires UI to core.
2. 🧪 `cargo test -p omacontrol-core` and `cargo clippy --workspace --all-targets -- -D warnings` at 0 before every push.
3. 🎨 **Omacalc is the design reference** (see CONTRIBUTING.md › Design rules). Don't add colors; mix the theme's two.
4. 🔐 TLS to TVs accepts self-signed certificates **only** for local TVs (`tv/tls.rs`). Never reuse that config for the internet.
5. 📴 Never block the UI thread: everything that touches the network or the IR blaster goes through the worker thread (`app/src/lib.rs`).
6. 🔖 **Bump, then always tag.** Set `[workspace.package] version` in `Cargo.toml` (and `Cargo.lock`), commit, then push the `vX.Y.Z` tag in the same breath — a bump without its tag is not a release. Pick patch/minor/major from the size of the change, and keep the `-beta` suffix unless the release is explicitly stable/latest, so `0.6.1-beta`, never `0.6.0-beta.2`. The Android workflow refuses a tag that doesn't match.
7. ✍️ Gitmoji commits, a CHANGELOG entry per version.
8. 🖥️ **A release is not tested until it runs on this machine.** The tag only produces artifacts; it never updates the copy you launch. After pushing it, wait for the "Android and Linux" run, then run `scripts/install_release.sh`: it takes the newest release (prereleases included), installs the binary to `~/.local/bin`, the icon and desktop entry to `~/.local/share`, and checks the installed digest against the tarball's. `omacontrol --version` says which build is installed.

## Where things are

| Task | Files |
|:-----|:------|
| Add a TV brand / protocol | `crates/omacontrol-core/src/tv/<brand>.rs`, `tv/mod.rs` (`Brand`, `connect`) |
| Add IR codes (a brand, an AC) | `crates/omacontrol-core/src/ir.rs` |
| Change an AC protocol or the AC remote | `crates/omacontrol-core/src/ac.rs`, `app/ui/ac.slint` |
| Refresh the Omarchy themes | `scripts/gen_themes.py` → `themes_gen.rs` |
| Change a screen | `app/ui/*.slint` |
| Android specifics (IR blaster, insets, manifest) | `app/src/android_ir.rs`, `app/src/lib.rs`, `app/Cargo.toml` (`[package.metadata.android]`) |
| The mic (hold / tap, TV audio stream or the phone's speech recognizer) | `app/src/voice.rs`, `app/src/android_voice.rs`, `app/src/android_speech.rs`, `app/android/java/…/Speech.java` (compiled to a dex by `app/build.rs`) |
| Pasted IR codes (Pronto, raw, Flipper `.ir`, LIRC) | `crates/omacontrol-core/src/ir_import.rs` |
| Install the newest release for testing here | `scripts/install_release.sh` |
