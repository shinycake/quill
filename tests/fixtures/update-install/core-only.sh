#!/bin/sh
# A core-only build of the new version: the right version, the wrong build.
case "$1" in
--version) echo 'Quill 999.0.0' ;;
*) echo core ;;
esac
