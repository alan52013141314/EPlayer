param([Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArgs)
$ErrorActionPreference = 'Stop'
$localRust = Join-Path $PSScriptRoot '..\..\tools\rust'
if (Test-Path -LiteralPath "$localRust\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = (Resolve-Path "$localRust\cargo").Path
    $env:RUSTUP_HOME = (Resolve-Path "$localRust\rustup").Path
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
Push-Location (Join-Path $PSScriptRoot '..')
try {
    & cargo @CargoArgs
    $result = $LASTEXITCODE
} finally { Pop-Location }
exit $result

