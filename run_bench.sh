#!/bin/bash
# Benchmark all key printpdf source files from smallest to largest
# Output goes entirely to bench_output/bench_all.log

cd /Users/fschutt/Development/git2pdf
mkdir -p bench_output
LOG=bench_output/bench_all.log
> "$LOG"

for f in utils.rs units.rs ops.rs font.rs lib.rs serialize.rs image.rs graphics.rs deserialize.rs; do
  echo "=== Benchmarking $f ===" >> "$LOG"
  fp="/Users/fschutt/Development/printpdf/src/$f"
  ./target/release/git2pdf --file "$fp" --output bench_output >> "$LOG" 2>&1
  echo "" >> "$LOG"
done

echo "=== ALL DONE ===" >> "$LOG"
date >> "$LOG"
