# scripts/dev.ps1 — local development one-shot (Windows / PowerShell)
#
# H-53 dev experience: copies backend/.env.example to backend/.env (if not
# already present), ensures Docker Compose PG/Redis is running, and starts
# the backend. The .env has the dev placeholders that pass H-53's format
# validation, so cargo run will boot.
#
# Usage: .\scripts\dev.ps1
#
# Stop the backend: Ctrl-C
# Stop the infra:   docker compose -f backend/docker-compose.yml down

$ErrorActionPreference = 'Stop'

# Resolve project root (one level up from this script).
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = (Resolve-Path "$ScriptDir\..").Path
Set-Location $ProjectRoot

# 1. Ensure backend/.env exists
$EnvPath = Join-Path $ProjectRoot 'backend\.env'
$EnvExamplePath = Join-Path $ProjectRoot 'backend\.env.example'
if (-not (Test-Path -Path $EnvPath -PathType Leaf)) {
    Write-Host "[dev.ps1] backend\.env not found; copying from .env.example"
    Copy-Item -Path $EnvExamplePath -Destination $EnvPath
} else {
    Write-Host "[dev.ps1] backend\.env already exists; leaving untouched"
}

# 2. Bring up Docker Compose (PG + Redis) if not already running
$DockerCmd = Get-Command docker -ErrorAction SilentlyContinue
if ($DockerCmd) {
    try {
        $DockerInfo = & docker info 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[dev.ps1] WARNING: Docker daemon not reachable; skipping compose up"
            Write-Host "[dev.ps1] Make sure Postgres (5433) + Redis (6379) are running before cargo run"
        } else {
            Write-Host "[dev.ps1] starting docker compose (PG:5433, Redis:6379)"
            & docker compose -f backend/docker-compose.yml up -d
        }
    } catch {
        Write-Host "[dev.ps1] WARNING: docker info failed: $_"
    }
} else {
    Write-Host "[dev.ps1] docker not on PATH; assuming PG/Redis are already running externally"
}

# 3. Run the backend (cargo not in PATH by default on Windows)
Set-Location (Join-Path $ProjectRoot 'backend')
$CargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (Test-Path -Path $CargoPath -PathType Leaf) {
    Write-Host "[dev.ps1] starting backend ($CargoPath run)..."
    & $CargoPath run
} else {
    Write-Host "[dev.ps1] cargo not found at $CargoPath; trying 'cargo run' directly"
    cargo run
}
