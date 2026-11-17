#!/usr/bin/env bash
# Installs a released OmaControl Linux build into ~/.local, so the launcher runs it.
#
#   scripts/install_release.sh            # the latest release
#   scripts/install_release.sh v0.6.1-beta  # a specific tag
#
# The release tarball holds the binary, the 512x512 icon and the desktop entry; all three
# go where a user install lives (~/.local), and the binary is checked against the tarball's
# digest so what you launch is exactly what CI built.
set -euo pipefail

repo="abraham-ukachi/omacontrol"
asset="omacontrol-linux-x86_64.tar.gz"
tag="${1:-}"
bin_dir="$HOME/.local/bin"
icon_dir="$HOME/.local/share/icons/hicolor/512x512/apps"
app_dir="$HOME/.local/share/applications"

if [ -z "$tag" ]; then
    # Not `releases/latest`: every release here is a `-beta` prerelease, and GitHub's
    # "latest" skips those. The release list is newest first, prereleases included.
    if command -v gh >/dev/null; then
        tag="$(gh api "repos/$repo/releases" --jq '.[0].tag_name')"
    else
        tag="$(curl -fsSL "https://api.github.com/repos/$repo/releases" |
            sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
    fi
fi
if [ -z "$tag" ]; then
    echo "could not work out the latest release tag" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "fetching $tag..."
if command -v gh >/dev/null; then
    gh release download "$tag" --repo "$repo" --pattern "$asset" --dir "$work"
else
    curl -fsSL -o "$work/$asset" \
        "https://github.com/$repo/releases/download/$tag/$asset"
fi

tar xzf "$work/$asset" -C "$work"

mkdir -p "$bin_dir" "$icon_dir" "$app_dir"
install -m 755 "$work/omacontrol" "$bin_dir/omacontrol"
install -m 644 "$work/omacontrol.png" "$icon_dir/omacontrol.png"
install -m 644 "$work/io.github.abraham_ukachi.omacontrol.desktop" \
    "$app_dir/io.github.abraham_ukachi.omacontrol.desktop"

if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$app_dir" >/dev/null 2>&1 || true
fi

built="$(sha256sum "$work/omacontrol" | cut -d' ' -f1)"
installed="$(sha256sum "$bin_dir/omacontrol" | cut -d' ' -f1)"
if [ "$built" != "$installed" ]; then
    echo "digest mismatch: installed $installed, release $built" >&2
    exit 1
fi

echo "installed $tag to $bin_dir/omacontrol (${installed:0:12})"
