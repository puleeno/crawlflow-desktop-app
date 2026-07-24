const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const srcTauriDir = path.resolve(__dirname, '../src-tauri');

// Get host target
let hostTarget = '';
try {
    const rustcV = execSync('rustc -vV').toString();
    const hostLine = rustcV.split('\n').find(line => line.startsWith('host:'));
    if (hostLine) {
        hostTarget = hostLine.split(':')[1].trim();
    }
} catch (e) {
    console.error('[build-service] Failed to get host target from rustc:', e.message);
}

const target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.TARGET || hostTarget;
if (!target) {
    console.error('[build-service] Could not determine target triple.');
    process.exit(1);
}

const isWindows = target.includes('-pc-windows-');
const binExt = isWindows ? '.exe' : '';

console.log(`[build-service] Stopping any running processes...`);
if (process.platform !== 'win32') {
    try {
        execSync('pkill -f "target/debug/crawlflow-service" 2>/dev/null || true');
        execSync('pkill -f "target/release/crawlflow-service" 2>/dev/null || true');
        execSync('pkill -f "binaries/crawlflow-service" 2>/dev/null || true');
        execSync('pkill -f "target/debug/crawlflow\\b" 2>/dev/null || true');
        execSync('pkill -f "target/release/crawlflow\\b" 2>/dev/null || true');
    } catch (e) { }
} else {
    try {
        execSync('taskkill /F /IM crawlflow-service.exe /T >nul 2>&1 || true');
        execSync('taskkill /F /IM crawlflow.exe /T >nul 2>&1 || true');
    } catch (e) { }
}

console.log(`[build-service] Compiling crawlflow-service for target: ${target}`);
let buildCmd = `cargo build --manifest-path "${path.join(srcTauriDir, 'Cargo.toml')}" --bin crawlflow-service --release`;
let srcBinPath = '';

if (target !== hostTarget) {
    buildCmd += ` --target ${target}`;
    srcBinPath = path.join(srcTauriDir, 'target', target, 'release', `crawlflow-service${binExt}`);
} else {
    srcBinPath = path.join(srcTauriDir, 'target/release', `crawlflow-service${binExt}`);
}

console.log(`Executing: ${buildCmd}`);
execSync(buildCmd, { stdio: 'inherit' });

// Create binaries directory
const binariesDir = path.join(srcTauriDir, 'binaries');
if (!fs.existsSync(binariesDir)) {
    fs.mkdirSync(binariesDir, { recursive: true });
}

// Clean up stale binaries
console.log(`[build-service] Cleaning stale service binaries...`);
try {
    const files = fs.readdirSync(binariesDir);
    for (const file of files) {
        if (file.startsWith('crawlflow-service-')) {
            fs.unlinkSync(path.join(binariesDir, file));
        }
    }
} catch (e) {
    console.warn('[build-service] Warning: Failed to clean stale binaries:', e.message);
}

const destBinPath = path.join(binariesDir, `crawlflow-service-${target}${binExt}`);
fs.copyFileSync(srcBinPath, destBinPath);
console.log(`[build-service] Copied to: ${destBinPath}`);
console.log(`[build-service] Done.`);
