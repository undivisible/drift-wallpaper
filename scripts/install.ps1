$ErrorActionPreference = "Stop"

$RepoUrl = $env:DRIFT_REPO_URL
if (-not $RepoUrl) {
    $RepoUrl = "https://github.com/undivisible/drift-wallpaper.git"
}

$Branch = $env:DRIFT_BRANCH
if (-not $Branch) {
    $Branch = "m"
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Install Rust from https://rustup.rs/ and re-run this script."
}

cargo install --git $RepoUrl --branch $Branch drift-app --locked --force
