#!/bin/sh
# Stays up until the close command (or the end of its input), as the real
# helper does. The close tests rely on it never exiting before that.
while read -r line; do
    case "$line" in *'"kind":"close"'*) exit 0 ;; esac
done
