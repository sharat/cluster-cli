#!/usr/bin/env bash
# Convert ANSI text screenshots to PNG images
# Uses ansisvg + rsvg-convert (librsvg) or other available tools

set -euo pipefail

INPUT_DIR="${1:-.}"
OUTPUT_DIR="${2:-$INPUT_DIR}"

echo "Converting ANSI screenshots to PNG..."
echo "Input:  $INPUT_DIR"
echo "Output: $OUTPUT_DIR"

# Check for available conversion tools
HAS_RSVG=false
HAS_ANSISVG=false
HAS_TERMSHOT=false

if command -v rsvg-convert &> /dev/null; then
    HAS_RSVG=true
    echo "Found: rsvg-convert"
fi

if command -v ansisvg &> /dev/null; then
    HAS_ANSISVG=true
    echo "Found: ansisvg"
fi

if command -v termshot &> /dev/null; then
    HAS_TERMSHOT=true
    echo "Found: termshot"
fi

# Function to convert using ansisvg + rsvg-convert
convert_ansisvg() {
    local input="$1"
    local output="$2"
    local svg="${output%.png}.svg"
    
    ansisvg < "$input" > "$svg"
    rsvg-convert "$svg" -o "$output"
    rm -f "$svg"
}

# Function to convert using termshot (if available)
# termshot render <INPUT> writes PNG to stdout
convert_termshot() {
    local input="$1"
    local output="$2"
    
    termshot render "$input" > "$output"
}

# Convert all .txt files
converted=0
for txtfile in "$INPUT_DIR"/*.txt; do
    if [ ! -f "$txtfile" ]; then
        continue
    fi
    
    basename=$(basename "$txtfile" .txt)
    pngfile="$OUTPUT_DIR/${basename}.png"
    
    echo "Converting: $basename"
    
    # Prefer ansisvg + rsvg-convert (more reliable in CI)
    if [ "$HAS_ANSISVG" = true ] && [ "$HAS_RSVG" = true ]; then
        convert_ansisvg "$txtfile" "$pngfile"
    elif [ "$HAS_TERMSHOT" = true ]; then
        convert_termshot "$txtfile" "$pngfile"
    else
        echo "  ERROR: No conversion tools available!"
        echo "  Install one of:"
        echo "    - termshot: cargo install termshot"
        echo "    - ansisvg + librsvg: npm install -g ansisvg && apt install librsvg2-bin"
        exit 1
    fi
    
    if [ -f "$pngfile" ]; then
        echo "  -> $pngfile"
        ((converted++))
    else
        echo "  -> FAILED"
    fi
done

echo ""
echo "Converted $converted files"
