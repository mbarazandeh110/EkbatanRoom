#!/usr/bin/env bash

set -euo pipefail

OUTPUT_FILE="all-rs-files.txt"

# فایل خروجی قبلی را حذف کن
: > "$OUTPUT_FILE"

file="proto/sfu/v1/media_engine.proto"
{
    echo "================================================================================"
    echo "FILE: $file"
    echo "================================================================================"
    cat "$file"
    echo
    echo
} >> "$OUTPUT_FILE"

find . -type f -name "*.rs" \
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
    } >> "$OUTPUT_FILE"
done

echo "Done."
echo "Output: $OUTPUT_FILE"
