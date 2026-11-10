# Terminal recording provenance

`terminal-session.ansi` is actual pseudoterminal output from Git and rustc in the
Linux development environment. It was captured on 2026-11-10 with:

```sh
TERM=xterm-256color LC_ALL=C script -q -c \
  'git --no-pager diff --no-index --color=always /dev/null src/lib.rs; rustc --version' \
  /dev/null > comparison/fixtures/terminal-session.ansi
```

Git emitted the colored diff; `script` captured terminal line discipline,
including CR/LF. The payload is a real command's output over this project's
source, not a manually composed ANSI fixture. It contains no private terminal
history, credentials, or user files. It captures the source at the time of
recording; later source edits naturally produce a different fresh recording.
The checked-in bytes remain fixed so everyone benchmarks the same input.

`sh comparison/record-terminal.sh` captures a new session when intentionally
refreshing this fixture. Record a new benchmark when changing the fixture.
