#!/usr/bin/env python3
"""Hosted checks: published dependency sources, a locked release build, and the tests.

The tests include the YouTube Music parser's fixtures (replies in the shape the service
sends), so a change on Google's side shows up here as a failing parser test. Never signs
in, never reaches YouTube."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
names = subprocess.check_output(['git', 'ls-files', '*Cargo.toml'], cwd=root, text=True).splitlines()
if any('file://' in (root / name).read_text() for name in names):
    raise SystemExit('Publish immutable shared dependency sources before enabling hosted CI.')


def run(*command):
    subprocess.run(command, cwd=root, check=True)


run('cargo', 'build', '--release', '--locked', '-p', 'nyx-lyra')
run('cargo', 'test', '--locked', '-p', 'nyx-lyra-core', '-p', 'nyx-lyra')
