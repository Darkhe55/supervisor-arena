#!/usr/bin/env bash
# scripts/dev.sh — local development one-shot
#
# H-53 dev experience: copies backend/.env.example to backend/.env (if not
# already present), ensures Docker Compose PG/Redis is running, and starts
# the backend. The .env has the dev placeholders that pass H-53's format
# validation, so cargo run will boot.
#
# Usage: ./scripts/dev.sh
#
# Stop the backend: Ctrl-C
# Stop the infra:   docker compose -f backend/docker-compose.yml down

set -euo pipefail

# Resolve project root (one level up from this script).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 1. Ensure backend/.env exists
if [[ ! -f backend/.env ]]; then
    echo "[dev.sh] backend/.env not found; copying from .env.example"
    cp backend/.env.example backend/.env
else
    echo "[dev.sh] backend/.env already exists; leaving untouched"
fi

# 2. Bring up Docker Compose (PG + Redis) if not already running
if command -v docker >/dev/null 2>&1; then
    if ! docker info >/dev/null 2>&1; then
        echo "[dev.sh] WARNING: Docker daemon not reachable; skipping compose up"
        echo "[dev.sh] Make sure Postgres (5433) + Redis (6379) are running before cargo run"
    else
        echo "[dev.sh] starting docker compose (PG:5433, Redis:6379)"
        docker compose -f backend/docker-compose.yml up -d
    fi
else
    echo "[dev.sh] docker not on PATH; assuming PG/Redis are already running externally"
fi

# 3. Run the backend
echo "[dev.sh] starting backend (cargo run)..."
cd backend
exec cargo run
