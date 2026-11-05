# Keep the crate version in step with the commit count, so cargo prints
# "Compiling amt-cli v0.1.NNN" instead of a fixed 0.1.0 that says nothing.
#
# PowerShell equivalent of scripts/sync-version.sh, for Windows checkouts
# without a POSIX shell. Nothing depends on this script at build time; it is
# only run by hand or wired into a local pre-commit hook if wanted.
#
# Up to 0.x the patch number was the commit count. From 1.0.0 on the version
# is decided by whoever writes the code: this script is a no-op there, same
# as the POSIX one.
$ErrorActionPreference = "Stop"

$root = git rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0) { exit 1 }
Set-Location $root

$manifest = "crates/amt-cli/Cargo.toml"
$version = (Select-String -Path $manifest -Pattern '^version = "(.*)"$' |
    Select-Object -First 1).Matches[0].Groups[1].Value

if ($version -notlike "0.*") {
    exit 0
}

$count = (git rev-list --count HEAD) + 1
$new = "0.1.$count"
if ($version -eq $new) { exit 0 }

# Only the first version line of the [package] block.
$done = $false
$lines = Get-Content $manifest | ForEach-Object {
    if (-not $done -and $_ -match '^version = "') {
        "version = `"$new`""
        $done = $true
    } else {
        $_
    }
}
Set-Content -Path $manifest -Value $lines
git add $manifest
Write-Output "sync-version: $version -> $new"
