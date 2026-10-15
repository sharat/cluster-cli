#!/usr/bin/env node
/**
 * Convert ANSI text files to PNG images
 * Usage: node ansi-to-png.js <input.txt> <output.png>
 * 
 * This script uses the ansi-to-svg library and rsvg-convert to render terminal screenshots.
 * Install deps: npm install -g ansi-to-svg && apt install librsvg2-bin
 */

const fs = require('fs');
const { execFileSync } = require('child_process');
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

// Node does not resolve globally installed packages, so fall back to `npm root -g`
function loadAnsiToSvg() {
    try {
        return require('ansi-to-svg');
    } catch (_) {
        const globalRoot = execFileSync('npm', ['root', '-g'], { encoding: 'utf-8' }).trim();
        return require(path.join(globalRoot, 'ansi-to-svg'));
    }
}

console.log(`Converting ${inputFile} -> ${outputFile}`);

const svgFile = outputFile.replace(/\.png$/, '') + '.svg';

try {
    const ansiToSvg = loadAnsiToSvg();
    const ansiText = fs.readFileSync(inputFile, 'utf-8');

    fs.writeFileSync(svgFile, ansiToSvg(ansiText));
    console.log(`  Created SVG: ${svgFile}`);

    execFileSync('rsvg-convert', [svgFile, '-o', outputFile], { stdio: 'inherit' });
    console.log(`  Created PNG: ${outputFile}`);
} catch (error) {
    console.error(`Error converting file: ${error.message}`);
    process.exitCode = 1;
} finally {
    fs.rmSync(svgFile, { force: true });
}
