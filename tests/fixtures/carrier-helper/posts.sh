#!/bin/sh
# Reads the open command, says it is ready and posts one page message, then
# stays up until the close command, as stays-up.sh does.
read -r line
echo '{"kind":"ready"}'
echo '{"kind":"text","text":"hi"}'
while read -r line; do
    case "$line" in *'"kind":"close"'*) exit 0 ;; esac
done
