#!/bin/sh
# Capture actual PTY output. The recorded program emits its own ANSI sequences.
set -eu
cd "$(dirname "$0")/.."
export TERM=xterm-256color
export LC_ALL=C
# `script` records the PTY's CR/LF processing; /dev/null avoids timestamp headers.
script -q -c 'git --no-pager diff --no-index --color=always /dev/null src/lib.rs; rustc --version' /dev/null > comparison/fixtures/terminal-session.ansi
