#!/bin/bash
cd "C:\Source\GitHub\graphtor" || cd /c/Source/GitHub/graphtor
echo "=== COMMAND 2: git rev-parse --abbrev-ref HEAD ==="
git rev-parse --abbrev-ref HEAD
echo "=== COMMAND 3: git rev-parse origin/main ==="
git rev-parse origin/main
echo "=== COMMAND 4: git merge-base origin/main HEAD ==="
git merge-base origin/main HEAD
echo "=== COMMAND 5: git diff --name-status origin/main...HEAD ==="
git diff --name-status origin/main...HEAD
echo "=== COMMAND 6: git diff --stat origin/main...HEAD ==="
git diff --stat origin/main...HEAD
echo "=== COMMAND 7: git log --oneline origin/main..HEAD ==="
git log --oneline origin/main..HEAD
echo "=== COMMAND 8: git status --porcelain=v1 ==="
git status --porcelain=v1
echo "=== COMMAND 9: git remote -v ==="
git remote -v
echo "=== SHA ea47df0 ==="
git cat-file -t ea47df0
git log -1 --format="%H %s" ea47df0
echo "=== SHA 75ff829 ==="
git cat-file -t 75ff829
git log -1 --format="%H %s" 75ff829
echo "=== SHA 6e207d7 ==="
git cat-file -t 6e207d7
git log -1 --format="%H %s" 6e207d7
echo "=== SHA 881fd66 ==="
git cat-file -t 881fd66
git log -1 --format="%H %s" 881fd66
echo "=== SHA f1b1007 ==="
git cat-file -t f1b1007
git log -1 --format="%H %s" f1b1007
echo "=== SHA af15470 ==="
git cat-file -t af15470
git log -1 --format="%H %s" af15470
echo "=== COMMAND 11: git ls-files .github/agents/ ==="
git ls-files .github/agents/
echo "=== COMMAND 12: git ls-files .backlogit/ ==="
git ls-files .backlogit/
echo "=== COMMAND 13: git check-ignore -v .autoharness/staging/.github/agents/.ship.agent.md ==="
git check-ignore -v ".autoharness/staging/.github/agents/.ship.agent.md"
