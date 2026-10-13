#!/usr/bin/env bash
# Convert ANSI text screenshots to PNG images
# Uses ansisvg + rsvg-convert (librsvg) or other available tools

set -uo pipefail

INPUT_DIR="${1:-.}"
OUTPUT_DIR="${2:-$INPUT_DIR}"

echo "Converting ANSI screenshots to PNG..."
echo "Input:  $INPUT_DIR"
echo "Output: $OUTPUT_DIR"

# Check for available conversion tools
HAS_RSVG=false
HAS_ANSITOSVG=false
HAS_TERMSHOT=false

if command -v rsvg-convert &> /dev/null; then
    HAS_RSVG=true
    echo "Found: rsvg-convert"
fi

if command -v ansi-to-svg &> /dev/null; then
    HAS_ANSITOSVG=true
    echo "Found: ansi-to-svg"
fi

if command -v termshot &> /dev/null; then
    HAS_TERMSHOT=true
    echo "Found: termshot"
fi

# Function to convert using ansi-to-svg + rsvg-convert
convert_ansisvg() {
    local input="$1"
    local output="$2"
    local svg="${output%.png}.svg"
    
    if ansi-to-svg < "$input" > "$svg" 2>/dev/null && \
       rsvg-convert "$svg" -o "$output" 2>/dev/null; then
        rm -f "$svg"
        return 0
    else
        rm -f "$svg" 2>/dev/null || true
        return 1
    fi
}

# Function to convert using termshot (if available)
# termshot render <INPUT> writes PNG to stdout
convert_termshot() {
    local input="$1"
    local output="$2"
    
    if termshot render "$input" > "$output" 2>/dev/null; then
        return 0
    else
        return 1
    fi
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
    
    # Try ansi-to-svg + rsvg-convert first (more reliable in CI)
    if [ "$HAS_ANSITOSVG" = true ] && [ "$HAS_RSVG" = true ]; then
        if convert_ansisvg "$txtfile" "$pngfile"; then
            success=true
        fi
    fi
    
    # Fall back to termshot if ansi-to-svg failed or wasn't available
    if [ "$success" = false ] && [ "$HAS_TERMSHOT" = true ]; then
        if convert_termshot "$txtfile" "$pngfile"; then
            success=true
        fi
    fi
    
    if [ "$success" = false ]; then
        echo "  ERROR: No conversion tools available or all tools failed!"
        echo "  Install one of:"
        echo "    - ansi-to-svg + librsvg: npm install -g ansi-to-svg && apt install librsvg2-bin"
        echo "    - termshot: cargo install termshot"
        ((failed++))
        continue
    fi
    
    if [ -f "$pngfile" ]; then
        echo "  -> $pngfile"
        ((converted++))
    else
        echo "  -> FAILED (file not created)"
        ((failed++))
    fi
done

echo ""
echo "Converted $converted files, $failed failed"

# Exit with error only if ALL conversions failed
if [ "$converted" -eq 0 ] && [ "$failed" -gt 0 ]; then
    exit 1
fi
