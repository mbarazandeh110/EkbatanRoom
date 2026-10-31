#!/usr/bin/env bash

set -euo pipefail

TEMP_FILE=$(mktemp)
trap 'rm -f "$TEMP_FILE"' EXIT

rm -f all-md-files-[0-9]*.txt

file="proto/sfu/v1/media_engine.proto"
{
    echo "================================================================================"
    echo "FILE: $file"
    echo "================================================================================"
    cat "$file"
    echo
    echo
} >> "$TEMP_FILE"

find . -type f -name "*.md" \
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

# all-md-files.txt
awk '{
    output = sprintf("all-md-files-%d.txt", int((NR - 1) / 2500) + 1)
    print >> output
}' "$TEMP_FILE"

echo "Done."
echo "Output files: all-md-files-1.txt, all-md-files-2.txt, ..."
