#!/usr/bin/env bash
# Signs an unsigned OmaControl.app with a distribution certificate and provisioning profile,
# then packs it as an .ipa. Usage: sign.sh <OmaControl.app> <out.ipa>
# Needs IOS_CERT_P12_BASE64, IOS_CERT_PASSWORD, IOS_PROVISION_PROFILE_BASE64 (CI secrets).
set -euo pipefail
app="$1"; out="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
work=$(mktemp -d)
keychain="$work/signing.keychain-db"
kc_pass=$(uuidgen)

printf '%s' "$IOS_CERT_P12_BASE64" | base64 --decode > "$work/cert.p12"
printf '%s' "$IOS_PROVISION_PROFILE_BASE64" | base64 --decode > "$work/profile.mobileprovision"

security create-keychain -p "$kc_pass" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$kc_pass" "$keychain"
security import "$work/cert.p12" -P "$IOS_CERT_PASSWORD" -A -t cert -f pkcs12 -k "$keychain"
security set-key-partition-list -S apple-tool:,apple: -k "$kc_pass" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
identity=$(security find-identity -v -p codesigning "$keychain" | awk 'NR==1 {print $2}')

security cms -D -i "$work/profile.mobileprovision" > "$work/profile.plist"
/usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$work/profile.plist" > "$work/entitlements.plist"

mkdir -p "$work/Payload"
cp -R "$app" "$work/Payload/"
signed="$work/Payload/$(basename "$app")"
cp "$work/profile.mobileprovision" "$signed/embedded.mobileprovision"
codesign --force --sign "$identity" --entitlements "$work/entitlements.plist" --timestamp=none "$signed"
codesign --verify --verbose "$signed"
(cd "$work" && zip -qry "$out" Payload)
security delete-keychain "$keychain" || true
