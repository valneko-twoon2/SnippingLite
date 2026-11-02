# GPU のない CI のランナーには OpenGL 2.0 がなく、egui の窓（履歴など）が開かない。ソフトウェアの
# OpenGL（Mesa の llvmpipe）の DLL を、E2E がアプリの写しを置く一時フォルダーへ入れて補う。
# アプリの置き場所の DLL が先に読まれるので、システムには手を入れない。
param([string]$Version = '26.2.5')
$ErrorActionPreference = 'Stop'
$folder = Join-Path ([IO.Path]::GetTempPath()) 'snippinglite-e2e-exe'
$work = Join-Path ([IO.Path]::GetTempPath()) 'mesa-download'
$null = New-Item -ItemType Directory -Path $folder, $work -Force
$archive = Join-Path $work "mesa3d-$Version-release-msvc.7z"
Invoke-WebRequest "https://github.com/pal1000/mesa-dist-win/releases/download/$Version/mesa3d-$Version-release-msvc.7z" -OutFile $archive
& 7z x $archive "-o$work\mesa" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw '7z failed' }
foreach ($name in 'opengl32.dll', 'libgallium_wgl.dll', 'libglapi.dll') {
    $found = Get-ChildItem "$work\mesa" -Recurse -Filter $name -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match 'x64' } | Select-Object -First 1
    if ($found) { Copy-Item $found.FullName $folder -Force; Write-Host "copied $name" }
    else { Write-Host "not in the archive: $name" }
}
if (-not (Test-Path (Join-Path $folder 'opengl32.dll'))) { throw 'opengl32.dll was not found' }
