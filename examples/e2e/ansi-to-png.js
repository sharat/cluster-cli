#!/usr/bin/env node
/**
 * Convert ANSI text files to PNG images
 * Usage: node ansi-to-png.js <input.txt> <output.png>
 * 
 * This script uses ansi-to-svg and sharp to render terminal screenshots.
 * Install deps: npm install -g ansi-to-svg sharp-cli
 */

const fs = require('fs');
const { execSync } = require('child_process');
const path = require('path');

if (process.argv.length < 4) {
    console.error('Usage: node ansi-to-png.js <input.txt> <output.png>');
    process.exit(1);
}

const inputFile = process.argv[2];
const outputFile = process.argv[3];

if (!fs.existsSync(inputFile)) {
    console.error(`Error: Input file not found: ${inputFile}`);
    process.exit(1);
}

console.log(`Converting ${inputFile} -> ${outputFile}`);

try {
    // Read ANSI text
    const ansiText = fs.readFileSync(inputFile, 'utf-8');
    
    // Create temp SVG file
    const svgFile = outputFile.replace(/\.png$/, '.svg');
    
    // Convert ANSI to SVG using ansi-to-svg
    execSync(`ansi-to-svg < "${inputFile}" > "${svgFile}"`, {
        encoding: 'utf-8',
        stdio: ['pipe', 'pipe', 'inherit']
    });
    
    console.log(`  Created SVG: ${svgFile}`);
    
    // Convert SVG to PNG using sharp-cli or rsvg-convert
    try {
        // Try sharp-cli first (better quality)
        execSync(`npx sharp-cli -i "${svgFile}" -o "${outputFile}" --format png`, {
            encoding: 'utf-8',
            stdio: 'inherit'
        });
    } catch (e) {
        // Fall back to rsvg-convert
        console.log('  sharp-cli not available, trying rsvg-convert...');
        execSync(`rsvg-convert "${svgFile}" -o "${outputFile}"`, {
            encoding: 'utf-8',
            stdio: 'inherit'
        });
    }
    
    console.log(`  Created PNG: ${outputFile}`);
    
    // Clean up temp SVG
    fs.unlinkSync(svgFile);
    
} catch (error) {
    console.error(`Error converting file: ${error.message}`);
    process.exit(1);
}
