#!/bin/sh
# The old version: records what it was started with in `started` beside the
# install.
echo "$@" > "$(dirname "$0")/../started"
