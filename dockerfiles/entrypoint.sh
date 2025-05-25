#!/usr/bin/env bash
set -e
. /home/esp/export-esp.sh
if [[ -n "$1" ]]; then
  exec "$@"
else
  exec bash
fi
