<p align="center">
  <!-- OmaControl - App Icon -->
  <a href="https://github.com/abraham-ukachi/omacontrol" target="_blank">
    <img src="./.github/omacontrol-icon-192.png" alt="OmaControl - App Icon" width="64" height="64" />
  </a>
</p>


<p align="center" style="width:512px; margin:0 auto;">
  <b>OmaControl</b> is a dead-simple remote for any smart TV (and soon, anything with an IR receiver), written in <b>Rust</b> 🦀 with <b>Omarchy</b>'s look & feel.
</p>

<p align="center" style="margin-top:12px;">
    <a href="https://github.com/abraham-ukachi/omacontrol/releases" target="_blank"><b>Download the APK &rarr;</b></a>
</p>




# `omacontrol`

> IMPORTANT: This is a work in progress and subject to major changes until version 1.0. Android first; Linux (Omarchy) next.


## What is OmaControl? 📺

Picture your living room 🛋️: the Samsung remote is under a cushion, the bedroom LG has its own, the Roku stick wants a third one, and the air conditioner 🥵 hides its remote in a drawer. That's a lot of remotes! 😅

**OmaControl** puts them all in your pocket 📱, in one calm, beautiful app that looks like it shipped with [Omarchy](https://omarchy.org). It finds the TVs on your Wi‑Fi 🔎, pairs with them once 🤝, and turns your phone into their remote: arrows, OK, volume, channels, media keys, streaming shortcuts (Netflix, YouTube, Prime...) and even **typing** ⌨️ into the TV's search box.

No Wi‑Fi on that old TV? If your phone has an **IR blaster** 🔴 (most Xiaomi, POCO & Redmi phones do), OmaControl speaks infrared too.

And it wears every **Omarchy theme** 🎨: Tokyo Night, Gruvbox, Rose Pine, Flexoki Light... 23 of them, painted exactly like [Omacalc](https://github.com/omacom-io/omacalc): two colors, a few mixes, iA Writer Mono, nothing else.

> MOTTO: We'll always do [**more**](#roadmap-) 😜


---


## Screens 📱

<img width="800" alt="OmaControl screens in Tokyo Night" src="./docs/design/omacontrol-screens-tokyo-night.png" />

| No. | Screen | What it does |
|:----|:-------|:-------------|
| 1 | *`TVs`* | Scans your network (SSDP), lists every TV found with its brand, **Add TV** by IP or over IR, **Rescan** |
| 2 | *`Remote`* | Power, input, mute, the D-pad with an inverted-ink **OK**, back / home / menu, **VOL** & **CH** rockers, rewind / play / fast-forward, keyboard & number pad shortcuts |
| 3 | *`Channels & apps`* | A big monospace channel display, streaming shortcuts, a number pad with CH / VOL keys, and **type to TV** |
| 4 | *`Menu`* | Jump between screens, and pick your Omarchy theme |

<img width="800" alt="The remote in Flexoki Light, Gruvbox and Rose Pine" src="./docs/design/omacontrol-remote-themes.png" />

> NOTE: These are the reference mockups (`docs/design`), rendered with the real font and theme colors.


## Supported devices 🔌

| No. | Device | How | Pairing | Status |
|:----|:-------|:----|:--------|:-------|
| 1 | *`Samsung`* (Tizen, 2016+) | Wi‑Fi · secure WebSocket `:8002` | press **Allow** on the TV once | ✅ 0.1.0 |
| 2 | *`LG`* (webOS) | Wi‑Fi · SSAP `:3001` / `:3000` + pointer socket | press **Yes** on the TV once | ✅ 0.1.0 |
| 3 | *`Roku`* / Roku TV | Wi‑Fi · ECP HTTP `:8060` | none | ✅ 0.1.0 |
| 4 | *`Samsung`*, *`LG`*, *`Sony`* | Infrared (phone's IR blaster) | none | ✅ 0.1.0 |
| 5 | *`Android TV`* / Google TV | Wi‑Fi · Android TV Remote v2 `:6466` | PIN shown on the TV | 🚧 0.2.0 |
| 6 | *`Air conditioners`* (fixed & portable) | Infrared (state-based codes) | none | 🗺️ 0.3.0 |


## Tech stack & versions 🧰

Versions as built on 2026-11-14:

| Tool | Version | Notes |
|:-----|:--------|:------|
| [Rust](https://www.rust-lang.org) | **stable** (1.100) | Edition 2021. Release profile: `opt-level = "s"`, LTO, stripped. |
| [Slint](https://slint.dev) | **1.x** | The UI (`app/ui/*.slint`). On Android: `backend-android-activity-06` + FemtoVG. |
| [tungstenite](https://github.com/snapview/tungstenite-rs) + [rustls](https://github.com/rustls/rustls) | **0.24** / **0.23** | WebSockets to Samsung & LG (self-signed TV certificates accepted on the LAN only). |
| [ureq](https://github.com/algesten/ureq) | **2.12** | Roku ECP & UPnP descriptions. |
| [jni](https://github.com/jni-rs/jni-rs) | **0.21** | Android's `ConsumerIrManager` (the IR blaster). |
| [cargo-apk](https://github.com/rust-mobile/cargo-apk) | latest | Builds the APK (see [`.github/workflows/android.yml`](./.github/workflows/android.yml)). |
| [iA Writer Mono](https://github.com/iaolo/iA-Fonts) | S | The font of Omacalc, bundled under the SIL OFL 1.1. |


## How it works 🦀⚡

| No. | Piece | File | What it does |
|:----|:------|:-----|:-------------|
| 1 | *`Discovery`* | [crates/omacontrol-core/src/discovery.rs](./crates/omacontrol-core/src/discovery.rs) | Sends SSDP `M-SEARCH` for each brand, tells them apart, reads the TV's friendly name |
| 2 | *`Protocols`* | [crates/omacontrol-core/src/tv/](./crates/omacontrol-core/src/tv) | One module per brand behind the same `Remote` trait (`press`, `type_text`, `launch`) |
| 3 | *`Infrared`* | [crates/omacontrol-core/src/ir.rs](./crates/omacontrol-core/src/ir.rs) | NEC, Samsung32, Sony SIRC & Pronto → pulse patterns, plus the TV code sets |
| 4 | *`Themes`* | [crates/omacontrol-core/src/theme.rs](./crates/omacontrol-core/src/theme.rs) | The 23 Omarchy themes and Omacalc's color mixing |
| 5 | *`Store`* | [crates/omacontrol-core/src/store.rs](./crates/omacontrol-core/src/store.rs) | Paired TVs, their tokens and your theme, in one JSON file |
| 6 | *`UI`* | [app/ui/](./app/ui) | The four screens, in Slint |
| 7 | *`App`* | [app/src/lib.rs](./app/src/lib.rs) | Wires the UI to the core; a worker thread does all the talking to TVs |
| 8 | *`IR blaster`* | [app/src/android_ir.rs](./app/src/android_ir.rs) | `ConsumerIrManager.transmit()` through JNI |


## Build & run 🛠️

```sh
cargo test -p omacontrol-core          # 20+ tests, no TV needed
cargo run -p omacontrol                # the UI on your desktop
cargo apk run --lib -p omacontrol      # on your Android phone (NDK + cargo-apk)
```

Every push to `main` builds an APK in GitHub Actions (artifact `omacontrol-apk`), and every `v*` tag attaches it to the release.


## Roadmap 🗺️

- [x] 📺 Samsung, LG & Roku over Wi‑Fi
- [x] 🔴 Samsung, LG & Sony over IR
- [x] 🎨 All Omarchy themes
- [ ] 🤖 Android TV / Google TV (Remote v2 + PIN pairing)
- [ ] ❄️ Air conditioners over IR: mode, temperature, fan, swing, timer (fixed & portable)
- [ ] 📚 More IR code sets (Pronto import, learn from an IR database)
- [ ] 🐧 OmaControl on Omarchy itself (follows the live theme from `~/.local/state/omarchy/current/theme/colors.toml`)
- [ ] 🔊 Soundbars, projectors, set-top boxes...


## More

Made with 🫶🏽 in Marseille by [Abraham Ukachi](https://github.com/abraham-ukachi). Design language from [Omarchy](https://omarchy.org) & [Omacalc](https://github.com/omacom-io/omacalc) by DHH and the Omarchy team (not affiliated).

License: [MIT](./LICENSE).
