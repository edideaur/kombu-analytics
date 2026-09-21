#!/bin/sh
set -e
HEALTH_URL="${KOMBU_HEALTH_URL:-http://localhost:3000/api/health}"
CHECK_INTERVAL="${WATCHDOG_INTERVAL_SECS:-10}"
MAX_FAILURES="${WATCHDOG_MAX_FAILURES:-3}"
FAILURES=0

while true; do
  sleep "$CHECK_INTERVAL"
  if curl -sf "$HEALTH_URL" > /dev/null 2>&1; then
    FAILURES=0
  else
    FAILURES=$((FAILURES + 1))
    echo "Watchdog: health check failed (${FAILURES}/${MAX_FAILURES})"
    if [ "$FAILURES" -ge "$MAX_FAILURES" ]; then
      echo "Watchdog: failure threshold exceeded, restarting service"
      exit 1
    fi
  fi
done
