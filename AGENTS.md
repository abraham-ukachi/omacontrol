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
6. ✍️ Gitmoji commits, a CHANGELOG entry per version.

## Where things are

| Task | Files |
|:-----|:------|
| Add a TV brand / protocol | `crates/omacontrol-core/src/tv/<brand>.rs`, `tv/mod.rs` (`Brand`, `connect`) |
| Add IR codes (a brand, an AC) | `crates/omacontrol-core/src/ir.rs` |
| Change an AC protocol or the AC remote | `crates/omacontrol-core/src/ac.rs`, `app/ui/ac.slint` |
| Refresh the Omarchy themes | `scripts/gen_themes.py` → `themes_gen.rs` |
| Change a screen | `app/ui/*.slint` |
| Android specifics (IR blaster, insets, manifest) | `app/src/android_ir.rs`, `app/src/lib.rs`, `app/Cargo.toml` (`[package.metadata.android]`) |
