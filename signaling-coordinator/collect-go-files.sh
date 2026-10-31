#!/usr/bin/env bash

set -euo pipefail

TEMP_FILE=$(mktemp)
trap 'rm -f "$TEMP_FILE"' EXIT

rm -f all-go-files-[0-9]*.txt

file="proto/sfu/v1/media_engine.proto"
{
    echo "================================================================================"
    echo "FILE: $file"
    echo "================================================================================"
    cat "$file"
    echo
    echo
} >> "$TEMP_FILE"

find . -type f -name "*.go" \
    ! -path "./.git/*" \
    ! -path "./target/*" \
    -print0 |
while IFS= read -r -d '' file; do
    {
        echo "================================================================================"
        echo "FILE: $file"
        echo "================================================================================"
        cat "$file"
        echo
        echo
    } >> "$TEMP_FILE"
done

# all-go-files.txt
awk '{
    output = sprintf("all-go-files-%d.txt", int((NR - 1) / 2500) + 1)
    print >> output
}' "$TEMP_FILE"

echo "Done."
echo "Output files: all-go-files-1.txt, all-go-files-2.txt, ..."
