#!/usr/bin/env python3
import subprocess
import sys
import os

os.chdir(r'C:\Source\GitHub\graphtor')

commands = [
    ('COMMAND 2: git rev-parse --abbrev-ref HEAD', 'git rev-parse --abbrev-ref HEAD'),
    ('COMMAND 3: git rev-parse origin/main', 'git rev-parse origin/main'),
    ('COMMAND 4: git merge-base origin/main HEAD', 'git merge-base origin/main HEAD'),
    ('COMMAND 5: git diff --name-status origin/main...HEAD', 'git diff --name-status origin/main...HEAD'),
    ('COMMAND 6: git diff --stat origin/main...HEAD', 'git diff --stat origin/main...HEAD'),
    ('COMMAND 7: git log --oneline origin/main..HEAD', 'git log --oneline origin/main..HEAD'),
    ('COMMAND 8: git status --porcelain=v1', 'git status --porcelain=v1'),
    ('COMMAND 9: git remote -v', 'git remote -v'),
    ('SHA ea47df0: git cat-file -t ea47df0', 'git cat-file -t ea47df0'),
    ('SHA ea47df0: git log -1 --format="%H %s" ea47df0', 'git log -1 --format=%H%n%s ea47df0'),
    ('SHA 75ff829: git cat-file -t 75ff829', 'git cat-file -t 75ff829'),
    ('SHA 75ff829: git log -1 --format="%H %s" 75ff829', 'git log -1 --format=%H%n%s 75ff829'),
    ('SHA 6e207d7: git cat-file -t 6e207d7', 'git cat-file -t 6e207d7'),
    ('SHA 6e207d7: git log -1 --format="%H %s" 6e207d7', 'git log -1 --format=%H%n%s 6e207d7'),
    ('SHA 881fd66: git cat-file -t 881fd66', 'git cat-file -t 881fd66'),
    ('SHA 881fd66: git log -1 --format="%H %s" 881fd66', 'git log -1 --format=%H%n%s 881fd66'),
    ('SHA f1b1007: git cat-file -t f1b1007', 'git cat-file -t f1b1007'),
    ('SHA f1b1007: git log -1 --format="%H %s" f1b1007', 'git log -1 --format=%H%n%s f1b1007'),
    ('SHA af15470: git cat-file -t af15470', 'git cat-file -t af15470'),
    ('SHA af15470: git log -1 --format="%H %s" af15470', 'git log -1 --format=%H%n%s af15470'),
    ('COMMAND 11: git ls-files .github/agents/', 'git ls-files .github/agents/'),
    ('COMMAND 12: git ls-files .backlogit/', 'git ls-files .backlogit/'),
    ('COMMAND 13: git check-ignore -v ".autoharness/staging/.github/agents/.ship.agent.md"', 'git check-ignore -v .autoharness/staging/.github/agents/.ship.agent.md'),
]

for label, cmd in commands:
    print(f"\n{label}")
    print("=" * 80)
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=10)
        if result.stdout:
            print(result.stdout.rstrip())
        if result.stderr:
            print(f"STDERR: {result.stderr.rstrip()}")
        if result.returncode != 0:
            print(f"Return code: {result.returncode}")
    except subprocess.TimeoutExpired:
        print("TIMEOUT")
    except Exception as e:
        print(f"ERROR: {e}")
