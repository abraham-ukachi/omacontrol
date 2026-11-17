# Changelog

All notable changes to this project will be documented in this file. Versions follow [SemVer](https://semver.org), see [CONTRIBUTING.md](./CONTRIBUTING.md#git-commit-messages) for the commit guidelines.

## [0.6.0-beta.2] - 2026-11-17

### Added
- :construction_worker: `scripts/check_translations.py` compares every `@tr()` screen string with the ten catalogs and fails on a translation that drops a `{}` placeholder, a duplicate msgid or an empty entry. CI runs it on every push and pull request.

### Changed
- :memo: README: the mic's hold and tap modes with the phone's speech fallback, the AC timer key, the four pasted code formats, Rust 1.99 and the build date.

### Fixed
- :pencil2: Two code comments: `show_ac`'s doc line sat above `apply_ac_key`, and the Daikin test's note trailed off mid-sentence.

## [0.6.0-beta.1] - 2026-11-17

### Added
- :walking: The mic has two modes: hold it to talk and let go to send (walkie-talkie, the default), or tap it to listen hands-free and tap again to send.
- :speaking_head: When the TV won't take the phone's audio, the phone listens instead: its own speech recognizer turns your words into text, then the TV's search opens and the text goes in. Each step shows in the status line.
- :joystick: A trackpad in place of the keyboard key: drag to move (one D-pad step every 40 px, so a long drag repeats), tap for OK, long-press for Back. It works on every TV, IR ones included.
- :1234: 123 opens a number pad in place of the volume and playback keys, with saved channels, a close key and the "Type to TV" row (it rides above the keyboard). No more separate page.
- :o: Swipe the D-pad left for a round one (a ring of four arrows around OK), right for the boxy keys. The choice is remembered.
- :alarm_clock: AC timers for Midea, Gree, TCL and Haier: 30-minute steps up to 24 h (TCL: 20 minutes up to 12 h). A running AC gets an off timer, a stopped one an on timer; it's saved with the time it was set, so restarts keep it right (#9).
- :inbox_tray: Import IR codes: long-press any AC key to paste a Pronto code, raw timings, a Flipper Zero `.ir` file (Flipper-IRDB) or LIRC codes. Named codes in a file land on their keys; bad or unsupported codes say why. AC setup takes the same formats (#9).

### Changed
- :card_file_box: The Archived list shows just the count, small, instead of a big "N archived" title.

### Fixed
- :bug: Android TV voice: the TV heard nothing because the phone's audio went out in 8 KiB pieces, which TVs take but recognize as silence. Audio now goes out in full 20 KiB messages (1.28 s), the last one padded.
- :bug: Android TV voice: tapping to send ended the session at once, which cancels the search. The phone now keeps the session open with silence until the TV says it heard the end of speech, then closes it a beat later, in the background.
- :bug: Android TV voice: a reconnect while starting voice no longer loses the TV's session id.

## [0.5.0] - 2026-11-16

### Added
- :microphone: The Android TV mic is a tap now: tap to talk, tap again to send, and it stops on its own after 12 s. The TV's Google Assistant takes the phone's audio.
- :robot: On an Android TV that can't take phone audio, the mic opens Google Assistant on the TV instead of showing an error.
- :keyboard: Typed text reaches Android TV fields through the TV's input method (IME): the whole text lands in the focused field, then Enter, so Go opens a URL in the browser or runs a search.

### Fixed
- :bug: Voice sessions that the TV numbers 0 were never seen, so the mic always timed out.
- :bug: Voice audio goes out in 8–20 KiB pieces, as Android TV expects; smaller pieces were dropped.
- :bug: The remote answers the TV's set-active with the features both sides agreed on.
- :bug: Text sent as key presses now covers URL signs (`: ? = _ & + # * ! %`) and ends with Enter.

## [0.4.2-beta.1] - 2026-11-16

### Fixed
- :bug: A new air conditioner is only saved once its setup finds a code; leaving setup early no longer leaves an empty "Air conditioner" in Devices.
- :wastebasket: Air conditioners left unset by earlier versions are cleared from Devices on launch.

## [0.4.1-beta.1] - 2026-11-16

### Added
- :sparkles: A magic key on IR TVs, in place of the second Home: tap Auto scan learns any code the TV answers to, long-press learns it again.
- :mag: Learning an AC key no longer stops at its own address: after its 254 commands it sweeps every command at `00`, `04` and `08`.

### Changed
- :gear: Settings is a list like Home (Theme, Language, Feedback, About); Theme and Language open a picker drawer from the bottom with the current choice pinned on top.
- :lipstick: The Add sheet groups the infrared choices under an "IR" title; the buttons now read Air conditioner, Samsung, LG and Sony.

### Fixed
- :bug: The remote's media keys sit in two even rows again (the mic or keyboard key no longer lands out of place).

## [0.4.0-beta.2] - 2026-11-16

### Added
- :alarm_clock: A Timer key on NEC ACs, left of Sleep and Swing; learn it like any other key.

### Changed
- :thermometer: NEC ACs now go from 16 °C to 31 °C.
- :pencil2: "Auto-find" is now "Auto scan".

### Fixed
- :bug: Opening an AC that's already set up (NEC or pasted code) goes straight to its keys instead of the new-AC setup.

## [0.4.0-beta.1] - 2026-11-16

### Added
- :brain: NEC ACs with only a power code now learn each key: an unlearned key shows `?`, sweeps codes on the power address, and saves the one you confirm.
- :mag: "Auto-find" is now a visible key on the AC setup page, next to Test.

### Fixed
- :bug: Move mode grabs the row: the dragged device follows your finger, lifts above the list and drops into its new slot.
- :bug: Swiping a device left or right slides the row and reveals Remove; let go past the mark to remove it.
- :bug: The last device is no longer hidden under the footer.
- :bug: Renaming keeps the device name as the title, adds Cancel, and lifts the drawer above the keyboard.
- :bug: "Found it" during a NEC scan steps back to the code the AC reacted to and replays it.
- :bug: The archived view shows the archived count and a restore hint instead of a duplicate button.

## [0.3.7] - 2026-11-16

### Added
- :iphone: Device list actions: swipe to remove with confirmation, long-press to rename/archive/move/remove, restore archived devices, and reorder in Move mode.
- :microphone: Press-and-hold voice search on Android TVs that negotiate microphone support; Android requests microphone permission before capture.
- :mag: NEC scan review: move to previous or next candidates and replay each one before saving the selected code.
- :earth_americas: A Settings language picker for system default or any of the ten bundled languages.

### Improved
- :keyboard: Keep Add-by-IP controls above the on-screen keyboard while entering an address.
- :globe_with_meridians: Translate the new device, voice-search, and NEC review UI into all ten bundled languages.

## [0.3.6] - 2026-11-16

### Fixed
- :bug: Android locale detection now returns the language tag after the JNI string borrow is dropped, allowing the APK to compile.

## [0.3.5] - 2026-11-16

### Added
- :earth_americas: Translated every app screen, built-in prompt and status message into German, Spanish, French, Hindi, Italian, Japanese, Portuguese, Russian, Turkish and Simplified Chinese.
- :iphone: Select the phone's current Android language or the Linux desktop locale on startup; unsupported languages use English.

### Fixed
- :bug: Locale bundles were built but not selected at startup, and previously only the AC setup flow had translated strings.

## [0.3.4] - 2026-11-16

### Added
- :snowflake: Long-press **Test** on AC setup to open a warning and run a cancellable 675-candidate NEC sweep. The first code is the reported `00FF40BF`; OmaControl cannot detect IR responses, so the user confirms a match while the code is displayed.
- :earth_americas: Slint system-locale translation bundles for the AC setup flow in German, Spanish, French, Hindi, Italian, Japanese, Portuguese, Russian, Turkish and Simplified Chinese.

### Changed
- :memo: Added localization as a tracked 1.0 release requirement; translations fall back to English when the system locale is not bundled.

## [0.3.3] - 2026-11-15

### Added
- :computer: An x86_64 Linux release bundle with an app-launcher entry and icon.

### Changed
- :memo: Documented Omarchy/Linux install requirements and live-theme fallback behavior.
- :construction_worker: Android workflow now builds and publishes the Linux desktop bundle alongside the APK.

## [0.3.2] - 2026-11-15

### Added
- :art: Linux can follow Omarchy's active `colors.toml` theme live, with built-in themes still selectable.
- :dart: A tracked 1.0 roadmap for Omarchy/Linux, LAN-connected computer controls, AV devices, and release gates.

### Changed
- :lock: Tagged Android releases now require the permanent signing key instead of silently publishing an APK with a throwaway key.
- :memo: Documented Android release-key setup and the 1.0 scope.

## [0.3.1] - 2026-11-15

### Changed
- :point_up_2: Saved channels lose their **edit** button: **long-press** a channel to rename it (the name field opens with the keyboard), **swipe it up** to delete it, after an "Are you sure?" dialog. Drag the chips sideways to scroll them.
- :keyboard: The **Pronto code** field and its **Test code** key now ride just above the keyboard while you type, like the Channels screen, and a long placeholder is cut with an ellipsis instead of overflowing.
- :arrow_up: GitHub Actions bumped (checkout v7, setup-java v6, upload-artifact v7, action-gh-release v3), clearing the Node 20 warnings.

### Fixed
- :bug: Tuning a saved channel sent its digits too fast, so the Freebox showed `402` or `42` instead of `4002`. Digits now go out 350 ms apart.
- :zap: Images optimized (Imgbot, #1).

## [0.3.0-beta.1] - 2026-11-15

### Added
- :robot: **Android TV / Google TV** over the Android TV Remote protocol v2 (the Freebox Player TV Free 4K included): the app makes its own RSA certificate once, pairs with the 6-character code the TV shows (typos are caught before anything is sent), then drives keys, typing and app shortcuts on port 6466, answering the TV's pings to stay connected. Tested end to end against a fake TV.
- :mag: Android TVs are found with mDNS (`_androidtvremote2._tcp`) next to SSDP.
- :tv: **Saved channels**: type a number, **+ save**, name it (`4002` → `BFMTV`); one tap tunes it. Kept per TV, and kept through rescans.
- :gear: **Settings** with three tabs: **Theme** (moved out of the menu), **About** (version, Abraham Ukachi, GitHub, X, email, source, license) and **Feedback** (feedback or issue, optional reply email, title, message), sent straight to the maintainer's inbox through Web3Forms.
- :sparkles: An **ASCII radar** (with a bouncing bar and a blinking cursor) plays while scanning for devices.
- :point_up_2: Tap a device's name at the top of the Remote, Channels or AC screen to go back to the devices list.

## [0.2.4] - 2026-11-15

### Fixed
- :bug: The APK reports the crate version again: the workspace was still on 0.2.0 while the tags had moved on, so every build since 0.2.1 identified itself as 0.2.0 to Android.

### Changed
- :construction_worker: A release tag must match the workspace version: the Android workflow stops before building when `vX.Y.Z` and `Cargo.toml` disagree.

## [0.2.3] - 2026-11-14

### Added
- :snowflake: Two more air conditioner protocols: **Haier** (YRW02, 14-byte frame) and **Daikin** (35-byte, three-section state frame), next to Midea, Coolix, Gree, TCL, LG, Electra, Samsung and Kelvinator.

## [0.2.2] - 2026-11-14

### Added
- :snowflake: Four more air conditioner protocols: **LG**, **Electra** (Electra, AUX, AEG Chillflex, De'Longhi Pinguino, Frigidaire), **Samsung** and **Kelvinator**, next to Midea, Coolix, Gree and TCL.
- A pasted **Pronto code** for air conditioners no protocol reaches: the setup screen tests it, and the power key sends it.

### Changed
- AC setup sends each code four frames (on, on, off, on): a weak phone blaster gets several tries, a unit that is already running beeps on the off frame, and the code leaves it on.

### Fixed
- The device store is written atomically and the previous file is kept as `omacontrol.json.bak`; an unreadable file moves to `omacontrol.json.corrupt` and the backup is loaded instead of silently starting empty.

## [0.2.1] - 2026-11-14

### Fixed
- Adding a TV by IP no longer freezes the app: the brand probe runs on the worker thread, behind a "looking for the TV…" status.
- Clippy is clean with `--all-targets` (the Midea test vectors' digit grouping).

### Changed
- CI lints test code too (`cargo clippy --workspace --all-targets`), and `AGENTS.md` documents the same command.
- The docs match 0.2.0: **Devices** instead of *TVs*, six screens, the air conditioner module, and Android TV as 0.3.0.

## [0.2.0-beta.1] - 2026-11-14

### Added
- :snowflake: Air conditioners over IR: Midea, Coolix, Gree and TCL protocols (most rebadged portables and splits, Cecotec included).
- A setup that tries each protocol until the AC beeps (phones can send IR but not receive it).
- The AC remote: power, mode, fan, − 22° +, sleep and swing, with the state saved between launches.
- The OK key launcher icon (adaptive on Android 8+).

### Changed
- The TVs screen is now **Devices**, and **Add TV** is **Add**.

## [0.1.0-beta.2] - 2026-11-14

### Added
- A faint haptic tick on every key, rocker and menu button.
- A short accent-colored pulse on each tap while an IR device is in use.
- Six launcher icon concepts in `design/icons/`.

### Fixed
- The "type to TV" row and GO now ride just above the on-screen keyboard while typing.

## 0.1.0 (2026-11-14)

* :tada: First version of **OmaControl**: a Rust + Slint remote for smart TVs, with Omarchy's look & feel (Omacalc is the reference)
* :tv: Three screens: **TVs** on your network, the **Remote** (D-pad, OK, back, home, menu, VOL / CH rockers, media keys) and **Channels & apps** (number pad, streaming shortcuts, type to the TV)
* :mag: TV discovery with SSDP (Samsung, LG, Roku, Android TV), or add a TV by IP
* :satellite: Protocols: Samsung Tizen (secure WebSocket, token pairing), LG webOS (SSAP + pointer socket, client key pairing), Roku (ECP)
* :red_circle: Infrared engine (NEC, Samsung32, Sony SIRC, Pronto hex) and the phone's IR blaster (`ConsumerIrManager`): Samsung, LG and Sony TVs over IR
* :art: All 23 Omarchy themes (generated from `omacom/omarchy`), colors mixed the Omacalc way, iA Writer Mono
* :construction_worker: CI (tests, check, clippy) and an Android workflow that builds the APK (attached to every `v*` release)
