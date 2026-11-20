#!/bin/bash

set -euo pipefail

BASE_SHA="${1:-}"
HEAD_SHA="${2:-}"

if [ -z "$BASE_SHA" ] || [ -z "$HEAD_SHA" ]; then
    echo "Usage: $0 <base_sha> <head_sha>"
    exit 1
fi

UNSIGNED=0
# An empty allowed-signers file lets git detect SSH signatures without trusting any key.
while IFS=' ' read -r commit status; do
    if [ "$status" = "N" ]; then
        echo "Commit $commit is not signed."
        UNSIGNED=$((UNSIGNED + 1))
    fi
done < <(git -c gpg.ssh.allowedSignersFile=/dev/null log --format="%H %G?" "${BASE_SHA}..${HEAD_SHA}")

if [ "$UNSIGNED" -gt 0 ]; then
    echo "Error: $UNSIGNED commit(s) are not signed. See CONTRIBUTING.md."
    exit 1
fi

echo "All commits are signed."
