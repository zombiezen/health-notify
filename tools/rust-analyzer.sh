#!/bin/sh
# Use the following VSCode setting:
# "rust-analyzer.server.path": "${workspaceFolder}/tools/rust-analyzer.sh"
exec direnv exec "$(dirname "$(dirname "$0")")" rust-analyzer "$@"
