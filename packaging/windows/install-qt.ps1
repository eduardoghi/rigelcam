# Installs Qt for MSVC into C:\Qt with aqtinstall, and stops when an archive it took isn't the one in
# qt-archives.sha256, or when one is missing or added.
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'

# Every package with its hash, so a file changed on PyPI stops the install.
python -m pip install --require-hashes -r (Join-Path $PSScriptRoot 'aqtinstall.txt')
if ($LASTEXITCODE -ne 0) { throw 'pip failed' }

$archives = Join-Path ([IO.Path]::GetTempPath()) 'qt-archives'
Remove-Item -Recurse -Force $archives -ErrorAction SilentlyContinue
python -m aqt install-qt windows desktop $Version win64_msvc2022_64 --outputdir C:\Qt --keep --archive-dest $archives
if ($LASTEXITCODE -ne 0) { throw 'aqt failed' }

$expected = @{}
foreach ($line in Get-Content (Join-Path $PSScriptRoot 'qt-archives.sha256')) {
    if ($line -match '^([0-9a-f]{64})  (\S+)$') { $expected[$Matches[2]] = $Matches[1] }
}
$taken = @(Get-ChildItem $archives -File)
if ($taken.Count -ne $expected.Count) {
    throw "aqt took $($taken.Count) archives of Qt and qt-archives.sha256 lists $($expected.Count)"
}
foreach ($archive in $taken) {
    # -ne ignores case, and Get-FileHash writes the hash in capitals.
    if ((Get-FileHash $archive.FullName -Algorithm SHA256).Hash -ne $expected[$archive.Name]) {
        throw "the Qt archive $($archive.Name) does not match its hash"
    }
}
Remove-Item -Recurse -Force $archives
