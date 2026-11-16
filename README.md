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

> IMPORTANT: This is a work in progress until version 1.0. Android is the first client; 1.0 adds an Omarchy/Linux desktop host and the LAN-connected device and computer controls below.


## What is OmaControl? 📺

Picture your living room 🛋️: the Samsung remote is under a cushion, the bedroom LG has its own, the Roku stick wants a third one, and the air conditioner 🥵 hides its remote in a drawer. That's a lot of remotes! 😅

**OmaControl** puts them all in your pocket 📱, in one calm, beautiful app that looks like it shipped with [Omarchy](https://omarchy.org). It finds the TVs on your Wi‑Fi 🔎 (Samsung, LG, Roku and **Android TV / Google TV**, the Freebox Player included), pairs with them once 🤝, and turns your phone into their remote: arrows, OK, volume, channels, media keys, streaming shortcuts (Netflix, YouTube, Prime...) and even **typing** ⌨️ into the TV's search box.

No Wi‑Fi on that old TV? If your phone has an **IR blaster** 🔴 (most Xiaomi, POCO & Redmi phones do), OmaControl speaks infrared too.

And it wears every **Omarchy theme** 🎨: Tokyo Night, Gruvbox, Rose Pine, Flexoki Light... 23 of them, painted exactly like [Omacalc](https://github.com/omacom-io/omacalc): two colors, a few mixes, iA Writer Mono, nothing else.

> MOTTO: We'll always do [**more**](#roadmap-) 😜


---


## Screens 📱

<img width="800" alt="OmaControl screens in Tokyo Night" src="./docs/design/omacontrol-screens-tokyo-night.png" />

| No. | Screen | What it does |
|:----|:-------|:-------------|
| 1 | *`Devices`* | Scans your network (SSDP + mDNS) behind an ASCII radar, lists every TV and air conditioner found, **Add** by IP or over IR, **Rescan** |
| 2 | *`Remote`* | Power, input, mute, the D-pad with an inverted-ink **OK**, back / home / menu, **VOL** & **CH** rockers, rewind / play / fast-forward, keyboard & number pad shortcuts. Android TVs ask for their **pairing code** here, once |
| 3 | *`Channels & apps`* | A big monospace channel display, **saved channels** (`4002` → **BFMTV**: type the number, **+ save**, name it; tap to tune, **long-press** to rename, **swipe up** to delete), streaming shortcuts, a number pad with CH / VOL keys, and **type to TV** |
| 4 | *`Menu`* | Devices, Remote, Channels & apps, Settings |
| 5 | *`AC`* | Air conditioners over IR: power, mode, fan, − 22° +, sleep & swing |
| 6 | *`AC setup`* | Tries each protocol or a pasted IR code; long-press **Test** for a warned, cancellable 675-candidate NEC sweep |
| 7 | *`Settings`* | **Theme** (all 23 Omarchy themes), **About** (version, author, links) and **Feedback** (an idea or an issue, sent straight to the maintainer's inbox) |

> TIP: Tap the device's name at the top of the Remote, Channels or AC screen to jump back to your devices.

<img width="800" alt="The remote in Flexoki Light, Gruvbox and Rose Pine" src="./docs/design/omacontrol-remote-themes.png" />

> NOTE: These are the reference mockups (`docs/design`), rendered with the real font and theme colors.


## Supported devices 🔌

| No. | Device | How | Pairing | Status |
|:----|:-------|:----|:--------|:-------|
| 1 | *`Samsung`* (Tizen, 2016+) | Wi‑Fi · secure WebSocket `:8002` | press **Allow** on the TV once | ✅ 0.1.0 |
| 2 | *`LG`* (webOS) | Wi‑Fi · SSAP `:3001` / `:3000` + pointer socket | press **Yes** on the TV once | ✅ 0.1.0 |
| 3 | *`Roku`* / Roku TV | Wi‑Fi · ECP HTTP `:8060` | none | ✅ 0.1.0 |
| 4 | *`Samsung`*, *`LG`*, *`Sony`* | Infrared (phone's IR blaster) | none | ✅ 0.1.0 |
| 5 | *`Air conditioners`* (fixed & portable): Midea, Coolix, Gree, TCL, LG, Electra, Samsung, Kelvinator, Haier and Daikin protocols (Cecotec, Comfee, Kaysun, Toshiba, Beko, TCL, AUX, AEG, De'Longhi, LG, Haier, Daikin...) | Infrared (state-based codes) | tap **Test** until the AC beeps, or paste a Pronto code | 🧪 0.2.2 |
| 6 | *`Android TV`* / Google TV (Freebox Player TV Free 4K, Chromecast with Google TV, Sony, TCL, Philips...) | Wi‑Fi · Android TV Remote v2 `:6467` pairing + `:6466` remote | type the 6-character code the TV shows, once | 🧪 0.3.0 |


## Tech stack & versions 🧰

Versions as built on 2026-11-15:

| Tool | Version | Notes |
|:-----|:--------|:------|
| [Rust](https://www.rust-lang.org) | **stable** (1.100) | Edition 2021. Release profile: `opt-level = "s"`, LTO, stripped. |
| [Slint](https://slint.dev) | **1.x** | The UI (`app/ui/*.slint`). On Android: `backend-android-activity-06` + FemtoVG. |
| [tungstenite](https://github.com/snapview/tungstenite-rs) + [rustls](https://github.com/rustls/rustls) | **0.24** / **0.23** | WebSockets to Samsung & LG (self-signed TV certificates accepted on the LAN only). |
| [ureq](https://github.com/algesten/ureq) | **2.12** | Roku ECP, UPnP descriptions, and the feedback form ([Web3Forms](https://web3forms.com)). |
| [rsa](https://github.com/RustCrypto/RSA) + [rcgen](https://github.com/rustls/rcgen) + [x509-parser](https://github.com/rusticata/x509-parser) | **0.9** / **0.13** / **0.16** | Android TV: our own RSA certificate, and the pairing secret from both certificates. |
| [jni](https://github.com/jni-rs/jni-rs) | **0.21** | Android's `ConsumerIrManager` (the IR blaster). |
| [cargo-apk](https://github.com/rust-mobile/cargo-apk) | latest | Builds the APK (see [`.github/workflows/android.yml`](./.github/workflows/android.yml)). |
| [iA Writer Mono](https://github.com/iaolo/iA-Fonts) | S | The font of Omacalc, bundled under the SIL OFL 1.1. |


## How it works 🦀⚡

| No. | Piece | File | What it does |
|:----|:------|:-----|:-------------|
| 1 | *`Discovery`* | [crates/omacontrol-core/src/discovery.rs](./crates/omacontrol-core/src/discovery.rs) | Sends SSDP `M-SEARCH` for each brand and an mDNS query for `_androidtvremote2._tcp`, tells them apart, reads the TV's friendly name |
| 2 | *`Protocols`* | [crates/omacontrol-core/src/tv/](./crates/omacontrol-core/src/tv) | One module per brand behind the same `Remote` trait (`press`, `type_text`, `launch`). Android TV ([`androidtv.rs`](./crates/omacontrol-core/src/tv/androidtv.rs)) speaks protobuf over TLS with its own certificate, tested end to end against a fake TV |
| 3 | *`Infrared`* | [crates/omacontrol-core/src/ir.rs](./crates/omacontrol-core/src/ir.rs) | NEC, Samsung32, Sony SIRC & Pronto → pulse patterns, plus the TV code sets |
| 4 | *`Air conditioners`* | [crates/omacontrol-core/src/ac.rs](./crates/omacontrol-core/src/ac.rs) | Midea, Coolix, Gree, TCL112, LG, Electra, Samsung, Kelvinator, Haier & Daikin state frames, a pasted Pronto code, and the state the remote keeps (IR is one way) |
| 5 | *`Themes`* | [crates/omacontrol-core/src/theme.rs](./crates/omacontrol-core/src/theme.rs) | The 23 Omarchy themes and Omacalc's color mixing |
| 6 | *`Store`* | [crates/omacontrol-core/src/store.rs](./crates/omacontrol-core/src/store.rs) | Paired TVs, their tokens, saved channels, the Android TV certificate and your theme, in one JSON file (saved atomically, with a backup) |
| 7 | *`Feedback`* | [crates/omacontrol-core/src/feedback.rs](./crates/omacontrol-core/src/feedback.rs) | Settings › Feedback: validates the form and posts it to Web3Forms, which emails it to the maintainer |
| 8 | *`UI`* | [app/ui/](./app/ui) | The seven screens (Devices, Remote, Channels & apps, Menu, AC, AC setup, Settings) and the ASCII scanner, in Slint |
| 9 | *`App`* | [app/src/lib.rs](./app/src/lib.rs) | Wires the UI to the core; a worker thread does all the talking to TVs, the IR blaster and the network |
| 10 | *`Android bits`* | [app/src/android_ir.rs](./app/src/android_ir.rs), [`haptics.rs`](./app/src/haptics.rs), [`android_url.rs`](./app/src/android_url.rs) | The IR blaster (`ConsumerIrManager`), the haptic tick, and opening links, through JNI |


## Build & run 🛠️

```sh
cargo test -p omacontrol-core          # Core tests, no TV needed
cargo test -p omacontrol-core --release -- --ignored   # + Android TV pairing against a fake TV
cargo run -p omacontrol                # the UI on your desktop
cargo apk run --lib -p omacontrol      # on your Android phone (NDK + cargo-apk)
```

Every push to `main` builds an APK (`omacontrol-apk`) and an x86_64 Linux bundle (`omacontrol-linux-x86_64`) in GitHub Actions. Tagged releases publish both assets; Android release builds require the permanent signing key to be configured as GitHub Actions secrets.

### Omarchy/Linux desktop

The x86_64 Linux release bundle includes the app, a desktop launcher and its icon. On Omarchy, download `omacontrol-linux-x86_64.tar.gz` from the [latest release](https://github.com/abraham-ukachi/omacontrol/releases/latest), then install it:

```sh
tar -xzf omacontrol-linux-x86_64.tar.gz
install -Dm755 omacontrol ~/.local/bin/omacontrol
install -Dm644 io.github.abraham_ukachi.omacontrol.desktop ~/.local/share/applications/io.github.abraham_ukachi.omacontrol.desktop
install -Dm644 omacontrol.png ~/.local/share/icons/hicolor/512x512/apps/omacontrol.png
```

The desktop entry makes OmaControl available in the app launcher. You can also launch it from a terminal with `omacontrol`. The bundle targets x86_64 Omarchy/Arch Linux and uses the system Wayland, Fontconfig and xkbcommon libraries.

To build and install from source, use a stable Rust toolchain and these Arch packages:

```sh
sudo pacman -S --needed base-devel pkgconf fontconfig libxkbcommon wayland
cargo install --locked --path app
```

Without an active Omarchy `colors.toml`, the desktop app keeps its built-in theme picker and uses Tokyo Night by default. When the file is available, **Omarchy (live)** follows its colors without a restart.

OmaControl follows the Android system language on phones and the desktop locale on Linux. All app screens and built-in prompts are translated into German, Spanish, French, Hindi, Italian, Japanese, Portuguese, Russian, Turkish and Simplified Chinese; English is used for English and unsupported locales. Operating-system and protocol diagnostic details may retain the language supplied by the underlying service.

The NEC sweep is an OmaControl-defined candidate set, not IrCode Finder's verified sequence. It begins with `00FF40BF`; because a phone cannot receive IR, watch the AC and tap **Found it** when it reacts.

After backing up the keystore and password in your vault, configure the repository secrets without printing either value:

```sh
base64 -w0 /path/to/omacontrol-release.p12 | gh secret set ANDROID_KEYSTORE_BASE64 --repo abraham-ukachi/omacontrol
gh secret set ANDROID_KEYSTORE_PASSWORD --repo abraham-ukachi/omacontrol < /path/to/omacontrol-release.pass
```


## Roadmap 🗺️

The [GitHub 1.0 milestone](https://github.com/abraham-ukachi/omacontrol/milestone/1) tracks delivery. The 1.0 host target is **Omarchy/Linux**; Windows and macOS hosts are out of scope for this release.

### Shipped Android foundation

- [x] 📺 Samsung, LG, Roku and Android TV / Google TV over Wi‑Fi
- [x] 🔴 Samsung, LG and Sony TVs over IR
- [x] 🎨 All 23 built-in Omarchy themes
- [x] ❄️ Air conditioners over IR: power, mode, temperature, fan, swing and sleep
- [x] 📺 Channels saved by name
- [x] ⚙️ Settings: theme, About and Feedback

### Required for 1.0

- [ ] 🐧 **Omarchy/Linux desktop app**: install and run on Omarchy; follow changes to `~/.local/state/omarchy/current/theme/colors.toml` without restarting.
- [ ] 🔐 **Paired Linux host**: a discoverable host component for phone-to-computer features. LAN-only for 1.0, with authenticated/encrypted sessions and explicit pairing and host approval; no cloud relay.
- [ ] 🖥️ **Remote desktop and display controls**: view and control the Omarchy desktop from the phone, including useful display controls such as brightness and monitor selection.
- [ ] 🖐️ **Phone as computer input**: keyboard, one-to-five-finger trackpad gestures, and gyroscope-driven air-mouse cursor control.
- [ ] 🎧 **Phone as computer audio device**: listen to Omarchy audio on the phone; send the phone microphone to the Linux host and, where supported, Android TV and other compatible TVs.
- [ ] 🔊 **More AV devices**: soundbars, projectors and set-top boxes, with a documented baseline command set and at least one tested representative device per supported family.
- [ ] 🎮 **Xbox and PlayStation 5**: provide supported control integrations; validate each console's available interfaces and document model- and capability-specific limits.
- [ ] ⏲️ **AC timers** and 📚 **broader IR code support**: timer controls plus practical Pronto import and IR database code lookup/import.
- [ ] ✅ **1.0 release gates**: document supported devices and limitations; validate pairing, recovery, responsiveness and core flows on real hardware; publish repeatable desktop and Android builds signed with the permanent release key.
- [x] 🌍 **Localization foundation**: select the system language automatically and translate all screens into the initial 10 languages.
- [ ] 🌍 **Localization expansion**: review translations and expand toward 50 languages.

The phone/host connection is deliberately local and user-approved in 1.0. Console and media-device integrations must use supported interfaces or documented IR codes; any device-specific limitations must be visible rather than implied to work universally.


## More

Made with 🫶🏽 in Marseille by [Abraham Ukachi](https://github.com/abraham-ukachi). Design language from [Omarchy](https://omarchy.org) & [Omacalc](https://github.com/omacom-io/omacalc) by DHH and the Omarchy team (not affiliated).

License: [MIT](./LICENSE).
