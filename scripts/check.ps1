# The quality gate, in one place.
#
#   pwsh scripts/check.ps1        writes: formats the code in place, then checks
#   pwsh scripts/check.ps1 -Ci    non-mutating twin: fails on unformatted code instead of fixing it
#
# CI runs this same script with -Ci, so the local gate and CI cannot disagree.

param(
    [switch]$Ci
)

$ErrorActionPreference = 'Stop'

function Invoke-Step([string]$Name, [scriptblock]$Command) {
    Write-Host "==> $Name"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    if ($Ci) {
        Invoke-Step 'format (verify)' { cargo fmt --all --check }
    }
    else {
        Invoke-Step 'format' { cargo fmt --all }
    }
    # --locked everywhere: a gate that quietly rewrites Cargo.lock has checked a different build.
    Invoke-Step 'clippy' { cargo clippy --workspace --all-targets --locked -- -D warnings }
    Invoke-Step 'test' { cargo test --workspace --locked }
}
finally { Pop-Location }

Write-Host 'check: clean'
