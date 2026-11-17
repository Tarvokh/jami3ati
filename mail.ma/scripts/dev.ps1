# Tira Mail — local dev helper (Windows, GNU toolchain).
#
# Usage:
#   .\scripts\dev.ps1 check
#   .\scripts\dev.ps1 test
#   .\scripts\dev.ps1 test-db    # needs .\scripts\pg-dev.ps1 first
#   .\scripts\dev.ps1 clippy
#   .\scripts\dev.ps1 fmt
#   .\scripts\dev.ps1 run
#
# Why this script exists:
#   This machine has no Visual Studio, so Rust runs on the
#   `stable-x86_64-pc-windows-gnu` toolchain. That toolchain still needs
#   GNU binutils (`as.exe`, `dlltool.exe`) and a `gcc` driver on PATH in
#   order to link. rustup's bundled `rust-mingw` component only ships a
#   stripped linker, and `dlltool` from it fails with
#   "dlltool could not create import library" / STATUS_DLL_NOT_FOUND.
#   So we point PATH at a real MinGW-w64 distribution.
param(
    [ValidateSet("check", "test", "test-db", "clippy", "fmt", "run", "clean")]
    [string]$Task = "check"
)

$ErrorActionPreference = "Stop"

# Candidate MinGW-w64 bin directories, most complete first.
$candidates = @(
    "C:\tools\winlibs\bin",
    "C:\tools\mingw64\mingw64\bin",
    "C:\tools\mingw64\bin"
)

$mingw = $candidates | Where-Object { Test-Path (Join-Path $_ "dlltool.exe") } | Select-Object -First 1

if (-not $mingw) {
    Write-Warning "No MinGW-w64 bin directory found. Linking will fail."
    Write-Warning "Install WinLibs, then re-run: winget install BrechtSanders.WinLibs.POSIX.UCRT"
} else {
    $env:Path = "$mingw;$env:Path"
    Write-Host "MinGW: $mingw"
}

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

switch ($Task) {
    "check"  { cargo check --workspace --all-targets }
    "test"   { cargo test --workspace }
    "test-db" {
        # Refuses to report a green run with zero storage coverage.
        if (-not $env:TIRA_TEST_DATABASE_URL) {
            $env:TIRA_TEST_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
        }
        $env:TIRA_REQUIRE_DATABASE = "1"
        cargo test --workspace
    }
    "clippy" { cargo clippy --workspace --all-targets -- -D warnings }
    "fmt"    { cargo fmt --all; cargo fmt --all -- --check }
    "run"    { cargo run -p tira-api }
    "clean"  { cargo clean }
}

if ($LASTEXITCODE -ne 0) {
    Write-Error "task '$Task' failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}
