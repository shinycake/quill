#!/bin/sh
# Reads the open command, says this system cannot run a web view, then
# waits for the end of its input.
read -r line
echo '{"kind":"failed","failure":{"kind":"unsupported"}}'
read -r line
