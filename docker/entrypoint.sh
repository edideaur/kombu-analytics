#!/bin/sh
set -e

echo "Applying database migrations..."
kombu migrate --database-url "$DATABASE_URL" ${ENGINE_ARGS:-}

echo "Starting Kombu server (engine=${STORAGE_ENGINE:-auto})..."
exec kombu serve --listen "${LISTEN:-0.0.0.0:3000}" ${ENGINE_ARGS:-}
