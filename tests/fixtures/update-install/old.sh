#!/bin/sh
# The old version: records what it was started with in `started` beside the
# install. It must exit on its own: the tests' starter waits for it.
echo "$@" > "$(dirname "$0")/../started"
