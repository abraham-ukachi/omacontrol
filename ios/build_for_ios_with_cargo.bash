#!/usr/bin/env bash
# From Slint's iOS guide (https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/),
# GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0, © SixtyFPS GmbH.
# Xcode runs it for the OmaControl target: cargo builds the binary, this script puts it in the .app.
set -euvx

# Fix up PATH to work around https://github.com/rust-lang/rust/issues/80817 and add cargo.
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH:$HOME/.cargo/bin"

if [[ "$CONFIGURATION" != "Debug" ]]; then
    CARGO_PROFILE=release
    MAYBE_RELEASE=--release
else
    CARGO_PROFILE=debug
    MAYBE_RELEASE=
fi

# Make Cargo output cache files in Xcode's directories
export CARGO_TARGET_DIR="${OMACONTROL_CARGO_TARGET_DIR:-$DERIVED_FILE_DIR/cargo}"

IS_SIMULATOR=0
if [ "${LLVM_TARGET_TRIPLE_SUFFIX-}" = "-simulator" ]; then
  IS_SIMULATOR=1
fi

executables=()
for arch in $ARCHS; do
    case "$arch" in
        arm64)
            if [ $IS_SIMULATOR -eq 0 ]; then
              CARGO_TARGET=aarch64-apple-ios
            else
              CARGO_TARGET=aarch64-apple-ios-sim
            fi
            ;;
        x86_64)
            export CFLAGS_x86_64_apple_ios="-target x86_64-apple-ios"
            CARGO_TARGET=x86_64-apple-ios
            ;;
    esac

    cargo build $MAYBE_RELEASE --target $CARGO_TARGET --bin "$1" "${@:2}"

    executables+=("$CARGO_TARGET_DIR/$CARGO_TARGET/$CARGO_PROFILE/$1")
done

# Combine executables, and place them at the output path excepted by Xcode
lipo -create -output "$TARGET_BUILD_DIR/$EXECUTABLE_PATH" "${executables[@]}"

# Force code signing every run for device builds (non-simulator), unless code
# signing is disabled (e.g. unsigned builds with CODE_SIGNING_ALLOWED=NO).
if [ $IS_SIMULATOR -eq 0 ] && [ "${CODE_SIGNING_ALLOWED:-YES}" != "NO" ] && [ -n "${EXPANDED_CODE_SIGN_IDENTITY:-}" ]; then
    ENTITLEMENTS_FILE="${TARGET_TEMP_DIR}/${PRODUCT_NAME}.app.xcent"
    if [ -s "$ENTITLEMENTS_FILE" ]; then
        codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY}" --entitlements "$ENTITLEMENTS_FILE" "${TARGET_BUILD_DIR}/${EXECUTABLE_PATH}"
    else
        codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY}" "${TARGET_BUILD_DIR}/${EXECUTABLE_PATH}"
    fi
fi
