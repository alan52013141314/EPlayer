param([Parameter(Mandatory=$true)][string]$FFmpeg)
$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot '..\verification\generated'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$formats = @(
    @('pcm16.wav','pcm_s16le'), @('pcm24.wav','pcm_s24le'), @('float.wav','pcm_f32le'),
    @('lossless.flac','flac'), @('cbr.mp3','libmp3lame'), @('audio.m4a','aac'),
    @('audio.aac','aac'), @('alac.m4a','alac'), @('vorbis.ogg','libvorbis'), @('opus.ogg','libopus')
)
foreach ($format in $formats) {
    & $FFmpeg -hide_banner -loglevel error -y -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=3' -ac 2 -c:a $format[1] -metadata 'title=EPlayer Test Tone' -metadata 'artist=Generated fixture' (Join-Path $out $format[0])
    if ($LASTEXITCODE -ne 0) { throw "Fixture generation failed: $($format[0])" }
}
& $FFmpeg -hide_banner -loglevel error -y -i (Join-Path $out 'pcm16.wav') -c:a libmp3lame -q:a 2 (Join-Path $out 'vbr.mp3')
if ($LASTEXITCODE -ne 0) { throw 'VBR generation failed' }
Write-Output "Fixtures generated in $out"
