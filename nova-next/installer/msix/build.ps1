# Builds the Microsoft Store package (MSIX) of Nova Island, like the appx target did for 1.7:
#   layout:  AppxManifest.xml (version filled in), assets\ (build\appx), app\Nova Island.exe
#   package: makeappx from the Windows SDK. Not signed: the Store signs it.
# Output: <target>\dist\Nova-Island-<version>-Store.msix (target: -Target, else CARGO_TARGET_DIR, else nova-next\target)
#   powershell -ExecutionPolicy Bypass -File nova-next\installer\msix\build.ps1
#   -Target <folder>   build there instead (when an antivirus locked a file in the usual one)
param([string]$Target = '')
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$next = Split-Path -Parent (Split-Path -Parent $here)
if (-not $Target) { $Target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $next 'target' } }
$root = Split-Path -Parent $next
$target = $Target
$env:CARGO_TARGET_DIR = $target

$version = (Select-String -Path "$next\crates\nova-app\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
Write-Host "Nova Island $version (Store)"

Push-Location $next
cargo build --release -p nova-app
if ($LASTEXITCODE) { throw 'cargo build failed' }
Pop-Location

$layout = "$target\msix\layout"
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force "$layout\assets", "$layout\app" | Out-Null
(Get-Content "$here\AppxManifest.xml" -Raw).Replace('{VERSION}', $version) | Set-Content "$layout\AppxManifest.xml" -Encoding utf8
Copy-Item "$root\build\appx\*.png" "$layout\assets\"
Copy-Item "$target\release\nova-island.exe" "$layout\app\Nova Island.exe"

$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory | Where-Object { $_.Name -match '^10\.' } | Sort-Object Name -Descending |
  ForEach-Object { "$($_.FullName)\x64\makeappx.exe" } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $sdk) { throw 'makeappx.exe not found (Windows SDK)' }
New-Item -ItemType Directory -Force "$target\dist" | Out-Null
$out = "$target\dist\Nova-Island-$version-Store.msix"
& $sdk pack /d $layout /p $out /o
if ($LASTEXITCODE) { throw 'makeappx failed' }

$f = Get-Item $out
Write-Host ("{0}  {1:N2} MB" -f $f.FullName, ($f.Length / 1MB))

# (1.8.7) the upload for Partner Center: the package and its symbols (.appxsym, a zip of the PDB).
# The Store then shows crash stacks with function names; upload this file instead of the .msix.
$sym = "$target\msix\symbols"
if (Test-Path $sym) { Remove-Item $sym -Recurse -Force }
New-Item -ItemType Directory -Force $sym | Out-Null
$appxsym = "$sym\Nova-Island-$version-Store.appxsym"
Compress-Archive -Path "$target\release\nova_island.pdb" -DestinationPath "$sym\pdb.zip" -Force
Move-Item "$sym\pdb.zip" $appxsym -Force
$upload = "$target\dist\Nova-Island-$version-Store.msixupload"
if (Test-Path $upload) { Remove-Item $upload -Force }
Compress-Archive -Path $out, $appxsym -DestinationPath "$sym\upload.zip" -Force
Move-Item "$sym\upload.zip" $upload -Force
$u = Get-Item $upload
Write-Host ("{0}  {1:N2} MB (package + symbols: upload this one)" -f $u.FullName, ($u.Length / 1MB))
