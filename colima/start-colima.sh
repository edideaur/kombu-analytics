#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROFILE="${COLIMA_PROFILE:-kombu}"

if ! command -v colima >/dev/null 2>&1; then
  echo "Error: colima CLI is not installed"
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "Error: docker CLI is not installed"
  exit 1
fi

if ! colima status -p "$PROFILE" >/dev/null 2>&1; then
  echo "Starting Colima instance with profile $PROFILE..."
  colima start "$PROFILE" --cpu 4 --memory 8 --disk 60 --vm-type vz --mount-type virtiofs
fi

echo "Deploying Kombu Analytics stack via Docker Compose..."
docker compose -p "$PROFILE" -f "$SCRIPT_DIR/docker-compose.colima.yml" up -d

echo "Waiting for Kombu health check on http://localhost:3000/api/health..."
for i in $(seq 1 30); do
  if curl -sf http://localhost:3000/api/health >/dev/null 2>&1; then
    echo "Kombu Analytics is healthy and ready on http://localhost:3000"
    exit 0
  fi
  sleep 2
done

echo "Warning: Kombu took longer than 60 seconds to become healthy"
docker compose -p "$PROFILE" -f "$SCRIPT_DIR/docker-compose.colima.yml" ps
exit 1
