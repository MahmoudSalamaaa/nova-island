# -Target: the Cargo target folder of the build to check (default: CARGO_TARGET_DIR, else nova-next\target)
param([string]$Target = $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot '..\..\target' }))
$t = "$Target\release\examples\wintool.exe"
$dev = Start-Process "$Target\release\nova-island.exe" -PassThru
Start-Sleep 6
function isl { (& $t island $dev.Id) -join ' ' }
$res = @("1. started: $(isl)")
New-Item -ItemType Directory -Force "$env:TEMP\nova-test" | Out-Null
$img = "$env:TEMP\nova-test\nova-fs-test.png"
Start-Process $img; Start-Sleep 5
$res += "2. photo: $((& $t key F11 Photos) -join ' ')"; Start-Sleep 3
$res += "2b. full-screen photo (front $((& $t front) -join ' ')): $(isl)  (want visible=false)"
$res += "    close: $((& $t key ALTF4 Photos) -join ' ')"; Start-Sleep 3
$res += "2c. after the photo: $(isl)  (want visible=true)"
& $t untop $dev.Id | Out-Null
$res += "3. untopped: $(isl)"
explorer.exe "$env:TEMP\nova-test"; Start-Sleep 3
$res += "3b. another app in front (front $((& $t front) -join ' ')): $(isl)  (want topmost=true)"
$res += "    close: $((& $t key ALTF4 explorer) -join ' ')"; Start-Sleep 1
Stop-Process -Id $dev.Id -Force
$res
