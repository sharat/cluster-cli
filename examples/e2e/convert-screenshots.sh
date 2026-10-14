#!/usr/bin/env bash
# Convert ANSI text screenshots to PNG images
# Uses termshot, or ansi-to-svg (Node library) + rsvg-convert (librsvg) as a fallback

set -uo pipefail

INPUT_DIR="${1:-.}"
OUTPUT_DIR="${2:-$INPUT_DIR}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Must match the tmux pane size used by test-views.sh
TERM_COLS="${TERM_COLS:-120}"
TERM_ROWS="${TERM_ROWS:-40}"

echo "Converting ANSI screenshots to PNG..."
echo "Input:  $INPUT_DIR"
echo "Output: $OUTPUT_DIR"

mkdir -p "$OUTPUT_DIR"

# Check for available conversion tools
HAS_TERMSHOT=false
HAS_NODE=false
HAS_RSVG=false

if command -v termshot &> /dev/null; then
    HAS_TERMSHOT=true
    echo "Found: termshot"
fi

if command -v node &> /dev/null; then
    HAS_NODE=true
    echo "Found: node"
fi

if command -v rsvg-convert &> /dev/null; then
    HAS_RSVG=true
    echo "Found: rsvg-convert"
fi

# Succeeds only if the file starts with the PNG signature
is_png() {
    [ -s "$1" ] && [ "$(head -c 8 "$1" | od -An -tx1 | tr -d ' \n')" = "89504e470d0a1a0a" ]
}

# termshot render writes a PNG to the -o path; without -o it only prints a path to stdout
convert_termshot() {
    local input="$1"
    local output="$2"

    termshot render --cols "$TERM_COLS" --rows "$TERM_ROWS" -o "$output" "$input" > /dev/null 2>&1
}

# ansi-to-svg has no CLI binary, so ansi-to-png.js calls it as a library
convert_ansisvg() {
    local input="$1"
    local output="$2"

    node "$SCRIPT_DIR/ansi-to-png.js" "$input" "$output" > /dev/null 2>&1
}

# Convert all .txt files
converted=0
failed=0
for txtfile in "$INPUT_DIR"/*.txt; do
    if [ ! -f "$txtfile" ]; then
        continue
    fi
    
    basename=$(basename "$txtfile" .txt)
    pngfile="$OUTPUT_DIR/${basename}.png"
    
    echo "Converting: $basename"
    
    success=false
    
    if [ "$HAS_TERMSHOT" = true ]; then
        if convert_termshot "$txtfile" "$pngfile" && is_png "$pngfile"; then
            success=true
        else
            rm -f "$pngfile"
        fi
    fi
    
    if [ "$success" = false ] && [ "$HAS_NODE" = true ] && [ "$HAS_RSVG" = true ]; then
        if convert_ansisvg "$txtfile" "$pngfile" && is_png "$pngfile"; then
            success=true
        else
            rm -f "$pngfile"
        fi
    fi
    
    if [ "$success" = true ]; then
        echo "  -> $pngfile"
        ((converted++))
    else
        echo "  ERROR: No conversion tools available or all tools failed!"
        echo "  Install one of:"
        echo "    - termshot: cargo install termshot"
        echo "    - ansi-to-svg + librsvg: npm install -g ansi-to-svg && apt install librsvg2-bin"
        ((failed++))
    fi
done

echo ""
echo "Converted $converted files, $failed failed"

# Exit with error only if ALL conversions failed
if [ "$converted" -eq 0 ] && [ "$failed" -gt 0 ]; then
    exit 1
fi
