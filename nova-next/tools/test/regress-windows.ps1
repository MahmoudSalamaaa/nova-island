# Window regression checks with wintool (no Add-Type: some antivirus programs block the DLLs it compiles).
# -Target: the Cargo target folder of the build to check (default: CARGO_TARGET_DIR, else nova-next\target)
param([string]$Target = $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot '..\..\target' }))
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$t = "$Target\release\examples\wintool.exe"
$dev = Start-Process "$Target\release\nova-island.exe" -PassThru
Start-Sleep 6
function isl { (& $t island $dev.Id) -join ' ' }
$res = @("1. started: $(isl)")
# Explorer in full screen keeps the island; a full-screen photo hides it; back, it returns
explorer.exe "$env:TEMP\nova-test"; Start-Sleep 3
$res += "2a. Explorer F11 ($((& $t key F11 explorer) -join ' ')): $(isl)  (want visible=true)"
Start-Sleep 2
New-Item -ItemType Directory -Force "$env:TEMP\nova-test" | Out-Null
$img = "$env:TEMP\nova-test\nova-fs-test.png"; if (-not (Test-Path $img)) { $b = New-Object System.Drawing.Bitmap 400,300; $b.Save($img); $b.Dispose() }
Start-Process $img; Start-Sleep 5
$res += "2b. full-screen photo (front $((& $t front) -join ' ')): $(isl)  (want visible=false)"
$res += "    close photo: $((& $t key ALTF4 Photos) -join ' ')"; Start-Sleep 3
$res += "2c. back in Explorer: $(isl)  (want visible=true)"
$res += "    Explorer out of full screen: $((& $t key F11 explorer) -join ' ')"; Start-Sleep 1
$res += "    close Explorer: $((& $t key ALTF4 explorer) -join ' ')"; Start-Sleep 2
# the screenshot overlay keeps the island
Start-Process 'ms-screenclip:'; Start-Sleep 3
$res += "3a. screenshot overlay (front $((& $t front) -join ' ')): $(isl)  (want visible=true)"
$res += "    cancel: $((& $t key ESC SnippingTool) -join ' ') $((& $t key ESC ScreenClippingHost) -join ' ')"; Start-Sleep 2
$res += "3b. after: $(isl)"
# on top again after losing its place
& $t untop $dev.Id | Out-Null
$f = New-Object System.Windows.Forms.Form; $f.Text = 'test app'; $f.Show(); $f.Activate()
for ($i = 0; $i -lt 20; $i++) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100 }
$res += "4. after another window: $(isl)  (want topmost=true)"
$f.Close()
# the island is in a screen copy (not excluded from captures)
$s = (& $t island $dev.Id)
$res += "5. widgets windows: $((python -I "$PSScriptRoot\widget-windows.py" | Measure-Object -Line).Lines)"
Stop-Process -Id $dev.Id -Force
$res
