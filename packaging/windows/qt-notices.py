"""Writes licenses/Qt-third-party.txt from the qt_attribution.json files of the Qt sources.

The Qt that aqtinstall puts on the runners has no notices for the code it bundles, so they come
from the sources of the same version. Only what goes into the Windows package is listed. Run it
again when QT_VERSION changes, with each module at the matching tag:

    git clone --depth 1 --branch v6.10.3 --filter=blob:none https://github.com/qt/qtbase.git
    (the same for qtdeclarative and qtsvg)
    python3 packaging/windows/qt-notices.py 6.10.3 qtbase qtdeclarative qtsvg
"""

import glob
import json
import os
import re
import sys

# The parts of Qt Core, Gui, Network, Qml, Quick, Quick Controls, Svg, the image plugins and the
# Windows platform plugin. The build system, tests, examples and other platforms are left out.
INCLUDED = {
    "d3d12memoryallocator", "vulkanmemoryallocator", "blake2", "doubleconversion", "easing",
    "emoji-segmenter", "freetype", "freetype-zlib", "freetype-bdf", "freetype-pcf",
    "harfbuzz-ng", "icc-srgb-color-profile", "libjpeg", "libpng", "psl-data", "libpsl", "md4",
    "md4c", "md5", "pcre2", "pcre2-sljit", "rfc6234", "sha1", "sha3_endian", "sha3_keccak",
    "siphash", "tinycbor", "wintab", "zlib", "tlexpected", "tika-mimetypes",
    "unicode-character-database", "unicode-cldr", "opengl-headers", "opengl-es2-headers",
    "grayraster", "smooth-scaling-algorithm", "xserverhelper", "rhi-miniengine-d3d12-mipmap",
    "aglfn", "vulkan-xml-spec", "webgradients", "masm", "yoga", "shadow_angular_material",
    "xsvg",
}

RULE = "=" * 100


def read(path):
    with open(path, encoding="utf-8", errors="replace") as file:
        return file.read().strip()


def as_list(value):
    if value is None:
        return []
    return [value] if isinstance(value, str) else list(value)


def license_texts(module, folder, entry):
    """The texts the entry points at, or the ones of its SPDX ids from LICENSES."""
    files = as_list(entry.get("LicenseFile")) + as_list(entry.get("LicenseFiles"))
    if files:
        return [read(os.path.join(folder, path)) for path in files]
    ids = re.findall(r"[A-Za-z0-9.+-]+", entry.get("LicenseId", ""))
    texts = []
    for spdx in ids:
        if spdx in ("AND", "OR", "WITH"):
            continue
        path = os.path.join(module, "LICENSES", spdx + ".txt")
        if os.path.exists(path):
            texts.append(read(path))
    return texts


def main():
    version, modules = sys.argv[1], sys.argv[2:]
    entries = []
    for module in modules:
        pattern = os.path.join(module, "**", "qt_attribution.json")
        for path in sorted(glob.glob(pattern, recursive=True)):
            with open(path, encoding="utf-8") as file:
                data = json.loads(file.read(), strict=False)
            for entry in data if isinstance(data, list) else [data]:
                if entry.get("Id") in INCLUDED:
                    entries.append((module, os.path.dirname(path), entry))
    found = {entry["Id"] for _, _, entry in entries}
    if found != INCLUDED:
        sys.exit(f"not in these sources: {sorted(INCLUDED - found)}")

    lines = [
        f"Third-party code inside the Qt {version} libraries and plugins of this package, as Qt",
        "lists it in its sources. Qt itself is under the GNU LGPL version 3, in LGPL-3.0.txt.",
    ]
    for module, folder, entry in sorted(entries, key=lambda item: item[2]["Name"].lower()):
        lines += ["", RULE, entry["Name"] + (f" {entry['Version']}" if entry.get("Version") else "")]
        if entry.get("Homepage"):
            lines.append(entry["Homepage"])
        lines.append("License: " + entry["LicenseId"])
        copyright = "\n".join(as_list(entry.get("Copyright")))
        if entry.get("CopyrightFile"):
            copyright = read(os.path.join(folder, entry["CopyrightFile"]))
        if copyright:
            lines += ["", copyright.strip()]
        for text in license_texts(module, folder, entry):
            lines += ["", text]
    output = os.path.join(os.path.dirname(__file__), "licenses", "Qt-third-party.txt")
    with open(output, "w", encoding="utf-8", newline="\n") as file:
        file.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
