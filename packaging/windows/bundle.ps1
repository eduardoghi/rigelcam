# Copies the app and what it needs to run into one folder, without Qt or GStreamer installed.
# Run it from a Visual Studio environment, since it needs dumpbin.
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Qt,
    [Parameter(Mandatory)][string]$Gstreamer,
    [Parameter(Mandatory)][string]$Output
)
$ErrorActionPreference = 'Stop'

# The full GStreamer has about 270 plugins and takes 2 GB. A plugin that is missing here shows up
# as an element the app reports at startup.
$plugins = 'app', 'coreelements', 'd3d11', 'isomp4', 'rtp', 'rtpmanager', 'rtsp', 'tcp', 'udp',
    'videoparsersbad'

$bin = Join-Path $Output 'bin'
$pluginFolder = Join-Path $Output 'lib\gstreamer-1.0'
$helperFolder = Join-Path $Output 'libexec\gstreamer-1.0'
foreach ($folder in $bin, $pluginFolder, $helperFolder) {
    New-Item -ItemType Directory -Force $folder | Out-Null
}

Copy-Item $Executable $bin
$qml = Join-Path $PSScriptRoot '..\..\crates\rigelcam-app\qml'
& (Join-Path $Qt 'bin\windeployqt.exe') --release --no-translations --no-opengl-sw `
    --no-compiler-runtime --qmldir $qml (Join-Path $bin (Split-Path $Executable -Leaf))
if ($LASTEXITCODE -ne 0) { throw 'windeployqt failed' }

# GStreamer finds its plugins from where its own DLL is, so this layout needs no variables.
foreach ($plugin in $plugins) {
    Copy-Item (Join-Path $Gstreamer "lib\gstreamer-1.0\gst$plugin.dll") $pluginFolder
}
Copy-Item (Join-Path $Gstreamer 'libexec\gstreamer-1.0\gst-plugin-scanner.exe') $helperFolder

# Copies what each file links to, and then what those link to.
$system = [Environment]::SystemDirectory
$sources = (Join-Path $Gstreamer 'bin'), (Join-Path $Qt 'bin')
$seen = @{}
$pending = [System.Collections.Generic.Queue[string]]::new()
Get-ChildItem $Output -Recurse -Include *.dll, *.exe | ForEach-Object { $pending.Enqueue($_.FullName) }
while ($pending.Count -gt 0) {
    $file = $pending.Dequeue()
    foreach ($line in & dumpbin /nologo /dependents $file) {
        if ($line -notmatch '^\s+(\S+\.dll)\s*$') { continue }
        $name = $Matches[1].ToLowerInvariant()
        if ($seen.ContainsKey($name)) { continue }
        $seen[$name] = $true
        if (Test-Path (Join-Path $bin $name)) { continue }
        $found = $sources | ForEach-Object { Join-Path $_ $name } | Where-Object { Test-Path $_ } |
            Select-Object -First 1
        # Windows has no Visual C++ runtime of its own to rely on.
        if (-not $found -and $name -match '^(vcruntime|msvcp)\d') { $found = Join-Path $system $name }
        if ($found) {
            Copy-Item $found $bin
            $pending.Enqueue((Join-Path $bin $name))
        }
    }
}

Copy-Item (Join-Path $PSScriptRoot '..\..\LICENSE') $Output
Copy-Item (Join-Path $PSScriptRoot 'THIRD-PARTY.txt') $Output
