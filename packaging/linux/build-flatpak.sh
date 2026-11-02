#!/bin/sh
# Builds a Flatpak bundle in the repository root. Needs Cargo, flatpak-builder or the app of the same
# name through FLATPAK_BUILDER, and Flathub as a remote of the user. Takes the name of the bundle as
# its argument. JOBS limits the compilers running at once.
set -eu
cd "$(dirname "$0")/../.."
id=io.github.eduardoghi.rigelcam
bundle=${1:-rigelcam.flatpak}
jobs=${JOBS:-$(nproc)}
builder=${FLATPAK_BUILDER:-flatpak-builder}

cargo vendor --locked --versioned-dirs packaging/linux/vendor > /dev/null
rm -rf target/flatpak/repo
$builder --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse --jobs="$jobs" \
    --repo=target/flatpak/repo \
    --state-dir=target/flatpak/state target/flatpak/build "packaging/linux/$id.yml"
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    target/flatpak/repo "$bundle" "$id"
