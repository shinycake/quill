#!/bin/sh
# The new version: answers both probes, and records what it was started
# with in `started` beside the install.
case "$1" in
--version) echo 'Quill 999.0.0' ;;
--build-info) echo ui ;;
*) echo "$@" > "$(dirname "$0")/../started" ;;
esac
