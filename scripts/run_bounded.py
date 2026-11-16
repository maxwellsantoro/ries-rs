#!/usr/bin/env python3
"""Run a command with Linux address-space and wall-time limits.

The OS enforces RLIMIT_AS on virtual address space, not RSS. This also limits
thread stacks and mappings. The child may abort on allocation failure. A failed
or timed-out search is incomplete and must never be treated as an empty result.
"""
import argparse
import os
import signal
import subprocess
import sys


def positive(value):
    n = int(value)
    if n <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return n


def run(command, memory_mib, seconds):
    if not sys.platform.startswith("linux"):
        raise ValueError("OS-enforced address-space budgeting is supported only on Linux")
    import resource

    def limits():
        cap = memory_mib * 1024 * 1024
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    process = subprocess.Popen(command, preexec_fn=limits, start_new_session=True)
    try:
        code = process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        print("Resource-limited run timed out; search is incomplete", file=sys.stderr)
        return 124
    if code:
        print(f"Resource-limited command failed ({code}); search is incomplete", file=sys.stderr)
    return code if code >= 0 else 128 - code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--memory-mib", type=positive, required=True)
    parser.add_argument("--seconds", type=positive, default=300)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required after --")
    try:
        return run(command, args.memory_mib, args.seconds)
    except (ValueError, OSError) as error:
        parser.exit(2, f"{error}\n")


if __name__ == "__main__":
    sys.exit(main())
