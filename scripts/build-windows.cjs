const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const projectDir = path.resolve(__dirname, '..');

// Prepend Homebrew's LLVM path on macOS to ensure cargo-xwin can find llvm-lib/lld/clang-cl
if (process.platform === 'darwin') {
    let llvmBinPath = '';
    try {
        const brewPrefix = execSync('brew --prefix llvm').toString().trim();
        llvmBinPath = path.join(brewPrefix, 'bin');
    } catch (e) {
        const siliconPath = '/opt/homebrew/opt/llvm/bin';
        const intelPath = '/usr/local/opt/llvm/bin';
        if (fs.existsSync(siliconPath)) {
            llvmBinPath = siliconPath;
        } else if (fs.existsSync(intelPath)) {
            llvmBinPath = intelPath;
        }
    }

    if (llvmBinPath && fs.existsSync(llvmBinPath)) {
        console.log(`[build-windows] Found LLVM at: ${llvmBinPath}. Adding to PATH.`);
        process.env.PATH = `${llvmBinPath}:${process.env.PATH}`;
    } else {
        console.warn('[build-windows] Warning: LLVM bin directory was not found.');
        console.warn('[build-windows] If the build fails with missing "llvm-lib", please run: brew install llvm');
    }
}

// 1. Ensure Rust target is installed
console.log('[build-windows] Step 1/4: Checking Rust Windows target...');
try {
    execSync('rustup target add x86_64-pc-windows-msvc', { stdio: 'inherit' });
} catch (e) {
    console.error('[build-windows] Failed to install rustup target x86_64-pc-windows-msvc:', e.message);
    process.exit(1);
}

// 2. Ensure cargo-xwin is installed
console.log('[build-windows] Step 2/4: Checking cargo-xwin installation...');
let hasXwin = false;
try {
    execSync('cargo xwin --version', { stdio: 'ignore' });
    hasXwin = true;
} catch (e) {
    // Not installed
}

if (!hasXwin) {
    console.log('[build-windows] Installing cargo-xwin via cargo (this might take a minute)...');
    try {
        execSync('cargo install cargo-xwin', { stdio: 'inherit' });
    } catch (e) {
        console.error('[build-windows] Failed to install cargo-xwin:', e.message);
        process.exit(1);
    }
} else {
    console.log('[build-windows] cargo-xwin is already installed.');
}

// 3. Build using cargo-xwin (abi3 build — no Python cross-lib needed)
console.log('[build-windows] Step 3/3: Starting Tauri release build for Windows target...');

process.env.CARGO = 'cargo-xwin';
process.env.TARGET = 'x86_64-pc-windows-msvc';

console.log(`[build-windows] CARGO=${process.env.CARGO}`);
console.log(`[build-windows] TARGET=${process.env.TARGET}`);

try {
    execSync('npm run tauri build -- --target x86_64-pc-windows-msvc', {
        cwd: projectDir,
        stdio: 'inherit',
        env: process.env
    });
    console.log('[build-windows] Build complete! File layout is in src-tauri/target/x86_64-pc-windows-msvc/release/bundle/');
} catch (e) {
    console.error('[build-windows] Build failed:', e.message);
    process.exit(1);
}
