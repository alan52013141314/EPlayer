param([string]$Label = 'candidate', [string]$ReleaseDirectory = 'target\release')
$ErrorActionPreference = 'Stop'
if ($Label -notmatch '^[A-Za-z0-9.-]+$') { throw 'Invalid package label' }
$root = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $root 'dist'
$output = Join-Path $dist "EPlayer-$Label-win-x64"
if (Test-Path -LiteralPath $output) { throw "Output already exists: $output. Use a new label." }
$release = if ([IO.Path]::IsPathRooted($ReleaseDirectory)) { $ReleaseDirectory } else { Join-Path $root $ReleaseDirectory }
$exe = Join-Path $release 'eplayer.exe'
$ffmpeg = Join-Path $root 'vendor\ffmpeg\ffmpeg-9.0.2-essentials_build'
foreach ($file in @($exe, (Join-Path $ffmpeg 'bin\ffmpeg.exe'), (Join-Path $ffmpeg 'bin\ffprobe.exe'))) {
    if (!(Test-Path -LiteralPath $file)) { throw "Missing input: $file" }
}
New-Item -ItemType Directory -Force -Path $output, "$output\tools", "$output\licenses" | Out-Null
Copy-Item -LiteralPath $exe -Destination "$output\eplayer.exe"
Copy-Item -LiteralPath "$ffmpeg\bin\ffmpeg.exe", "$ffmpeg\bin\ffprobe.exe" -Destination "$output\tools"
Copy-Item -LiteralPath "$ffmpeg\LICENSE" -Destination "$output\licenses\FFmpeg-GPLv3.txt"
Copy-Item -LiteralPath "$ffmpeg\README.txt" -Destination "$output\licenses\FFmpeg-build-README.txt"
Copy-Item -LiteralPath "$root\docs\USER-GUIDE.md" -Destination "$output\使用說明.md"
Copy-Item -LiteralPath "$root\docs\ACCEPTANCE.md" -Destination "$output\驗收報告.md"
Push-Location $root
try {
    $metadataText = & "$PSScriptRoot\cargo.ps1" -CargoArgs @('metadata','--offline','--format-version','1','--filter-platform','x86_64-pc-windows-msvc')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = ($metadataText -join "`n") | ConvertFrom-Json
    $reachable = @{}
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($metadata.resolve.root)
    while ($pending.Count -gt 0) {
        $id = $pending.Dequeue()
        if ($reachable.ContainsKey($id)) { continue }
        $reachable[$id] = $true
        $node = $metadata.resolve.nodes | Where-Object id -eq $id
        foreach ($dep in $node.dependencies) { $pending.Enqueue($dep) }
    }
    $notices = [Collections.Generic.List[string]]::new()
    $notices.Add('# EPlayer third-party dependencies')
    $notices.Add('FFmpeg 9.0.2 essentials build: https://www.gyan.dev/ffmpeg/builds/ ; source: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz')
    $notices.Add('FFmpeg runs as a separate local process. Its GPLv3 license and build configuration are included in licenses/.')
    foreach ($package in ($metadata.packages | Sort-Object name,version)) {
        if (!$reachable.ContainsKey($package.id) -or $package.id -eq $metadata.resolve.root) { continue }
        $dir = Split-Path $package.manifest_path -Parent
        $name = "$($package.name)-$($package.version)"
        $notices.Add("`n## $name`nLicense: $($package.license)`nRepository: $($package.repository)")
        $licenseFiles = Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|AUTHORS)' }
        if ($package.name -eq 'eframe' -and !$licenseFiles) { throw 'Patched eframe license texts are missing' }
        if ($licenseFiles) {
            New-Item -ItemType Directory -Path "$output\licenses\$name" | Out-Null
            $licenseFiles | Copy-Item -Destination "$output\licenses\$name"
            if ($package.name -eq 'eframe') {
                Copy-Item -LiteralPath "$dir\EPLAYER-PATCH.md" -Destination "$output\licenses\$name"
            }
        }
    }
    $notices | Set-Content -LiteralPath "$output\THIRD-PARTY-NOTICES.md" -Encoding utf8
} finally { Pop-Location }
$manifest = Get-ChildItem -LiteralPath $output -Recurse -File | ForEach-Object {
    [ordered]@{path=$_.FullName.Substring($output.Length+1).Replace('\','/');bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$output\manifest.json" -Encoding utf8
Compress-Archive -LiteralPath $output -DestinationPath "$output.zip" -CompressionLevel Optimal
Get-FileHash -LiteralPath "$output.zip" -Algorithm SHA256 | Format-List
Write-Output "Package: $output.zip"
