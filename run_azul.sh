#!/bin/bash
set -e

OUTPUT_DIR="./azul_output"
LOG_FILE="./azul_output/run.log"
SOURCE="/Users/fschutt/Development/azul"

mkdir -p "$OUTPUT_DIR"

echo "Starting git2pdf for azul crates..."
echo "Output dir: $OUTPUT_DIR"
echo "Log file: $LOG_FILE"
echo ""

for CRATE in azul-dll; do
    echo "Processing $CRATE ..."
    ./target/release/git2pdf "$SOURCE" \
        --crates "$CRATE" \
        --verbose \
        -o "$OUTPUT_DIR" \
        >> "$LOG_FILE" 2>&1
    echo "  Done: $CRATE"
done

echo ""
echo "All crates finished. PDFs in $OUTPUT_DIR"
ls -lhS "$OUTPUT_DIR"/*.pdf 2>/dev/null
