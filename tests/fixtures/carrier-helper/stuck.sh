#!/bin/sh
# Ignores every command, as a helper whose loop is stuck: only a kill ends
# it, or, so that none is left behind, the end of the process that started
# it.
while kill -0 "$PPID" 2>/dev/null; do
    sleep 1
done
