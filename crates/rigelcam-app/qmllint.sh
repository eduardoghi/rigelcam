#!/usr/bin/env bash
# Runs qmllint on qml/Main.qml and fails on any warning. Needs a build of rigelcam-app first, since
# the types that come from Rust are in the QML module the build writes. The app registers the types
# of cpp/video_item.h itself, so Qt has nothing that describes them, and moc and qmltyperegistrar
# write that here from a copy of the header.
set -euo pipefail
cd "$(dirname "$0")"

query() { qmake6 -query "$1" 2>/dev/null || qmake -query "$1"; }
libexec=$(query QT_INSTALL_LIBEXECS)
metatypes=$(query QT_INSTALL_ARCHDATA)/metatypes

module=$(find ../../target -path '*/out/qt-build-utils/qml_modules/rigelcam' -type d -printf '%T@ %p\n' 2>/dev/null |
    sort -n | tail -1 | cut -d' ' -f2-)
if [ -z "$module" ]; then
    echo "build rigelcam-app first, qmllint reads the QML module of the build" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/rigelcam/video"
# The qmldir of the build names its files under qml/, which are in the resources of the app.
cp "$module/qmldir" "$module/plugin.qmltypes" "$work/rigelcam/"
ln -s "$PWD/qml" "$work/rigelcam/qml"

# A copy with QML_NAMED_ELEMENT on each type, which is all qmltyperegistrar looks for. The header of
# the app goes without it, since moc would then need the full declaration of every type it names.
sed -E 's/^(class (VideoItem|Retranslator|GraphicsSetup|HdrPass) : public \w+ \{)$/\1 QML_NAMED_ELEMENT(\2)/' \
    cpp/video_item.h >"$work/video_item.h"
sed -i '1i #include <QtQml/qqmlregistration.h>' "$work/video_item.h"
# shellcheck disable=SC2046 # one argument per include path
"$libexec/moc" $(pkg-config --cflags-only-I Qt6Quick) --output-json "$work/video_item.h" -o "$work/moc.cpp"
foreign=$(printf '%s,' "$metatypes"/qt6{core,gui,qml,quick}_metatypes.json)
"$libexec/qmltyperegistrar" --import-name=rigelcam.video --major-version=1 \
    --foreign-types="${foreign%,}" --generate-qmltypes="$work/rigelcam/video/plugin.qmltypes" \
    -o "$work/types.cpp" "$work/moc.cpp.json"
printf 'module rigelcam.video\ntypeinfo plugin.qmltypes\ndepends QtQuick auto\n' >"$work/rigelcam/video/qmldir"

"$(query QT_INSTALL_BINS)/qmllint" --max-warnings 0 -I "$work" qml/Main.qml
