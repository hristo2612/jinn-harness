#!/usr/bin/env python3
"""Detached-process launcher for the soak (SOAK.md).

macOS ships no `setsid` binary, and a plain background job stays in the
launching shell's process group — a later group-directed kill (the known
gateway-restart hazard) would take the soak down with it. This launcher
forks, calls `os.setsid()` in the child so it leads a fresh session with
no controlling terminal, redirects stdio to a log file, and execs the
command. The parent prints the child's pid and exits.

usage: detach.py <logfile> <command> [args...]
"""

import os
import sys


def main() -> None:
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__)
        sys.exit(2)
    logpath, command = sys.argv[1], sys.argv[2:]
    pid = os.fork()
    if pid == 0:
        os.setsid()
        log = os.open(logpath, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        null = os.open(os.devnull, os.O_RDONLY)
        os.dup2(null, 0)
        os.dup2(log, 1)
        os.dup2(log, 2)
        os.execvp(command[0], command)
    print(pid)


if __name__ == "__main__":
    main()
