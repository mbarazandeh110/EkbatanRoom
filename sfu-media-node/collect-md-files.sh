#!/usr/bin/env bash

set -euo pipefail

OUTPUT_FILE="all-md-files.txt"
CHUNK_SIZE=2500

# حذف فایل‌های خروجی قبلی
rm -f "${OUTPUT_FILE}" "${OUTPUT_FILE}".part-*

file="proto/sfu/v1/media_engine.proto"

{
    echo "================================================================================"
    echo "FILE: $file"
    echo "================================================================================"
    cat "$file"
    echo
    echo
} >> "$OUTPUT_FILE"

# تجمیع تمام فایل‌های Markdown
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
    } >> "$OUTPUT_FILE"
done

# تقسیم فایل خروجی به فایل‌های ۲۵۰۰ خطی
split -l "$CHUNK_SIZE" \
    -d \
    -a 4 \
    --additional-suffix=.txt \
    "$OUTPUT_FILE" \
    "${OUTPUT_FILE}.part-"

# حذف فایل اصلی پس از تقسیم
rm -f "$OUTPUT_FILE"

echo "Done."
echo "Chunk size: $CHUNK_SIZE lines"
echo "Generated files:"

ls -lh "${OUTPUT_FILE}".part-*
