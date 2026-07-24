const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const https = require('https');

const projectDir = path.resolve(__dirname, '..');
const srcTauriDir = path.join(projectDir, 'src-tauri');
const libsDir = path.join(srcTauriDir, 'libs');
const pythonWindowsDir = path.join(libsDir, 'python-windows');

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

// 3. Download Python Windows development libraries for PyO3 cross-compilation
console.log('[build-windows] Step 3/4: Setting up Python Windows libraries for PyO3...');

function downloadFile(url, dest, callback) {
    https.get(url, (response) => {
        if (response.statusCode === 301 || response.statusCode === 302) {
            downloadFile(response.headers.location, dest, callback);
            return;
        }
        if (response.statusCode !== 200) {
            callback(new Error(`Failed to download: Status Code ${response.statusCode}`));
            return;
        }
        const file = fs.createWriteStream(dest);
        response.pipe(file);
        file.on('finish', () => {
            file.close();
            callback(null);
        });
    }).on('error', (err) => {
        fs.unlink(dest, () => { });
        callback(err);
    });
}

function triggerBuild() {
    console.log('[build-windows] Step 4/4: Starting Tauri release build for Windows target...');

    process.env.CARGO = 'cargo-xwin';
    process.env.PYO3_CROSS_LIB_DIR = pythonWindowsDir;
    process.env.TARGET = 'x86_64-pc-windows-msvc';

    console.log(`[build-windows] PYO3_CROSS_LIB_DIR=${process.env.PYO3_CROSS_LIB_DIR}`);
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
}

if (fs.existsSync(pythonWindowsDir)) {
    console.log('[build-windows] Python Windows libraries already active at:', pythonWindowsDir);
    triggerBuild();
} else {
    const version = '3.9.13';
    const nugetUrl = `https://www.nuget.org/api/v2/package/python/${version}`;
    const tmpFilePath = path.join('/tmp', `python-${version}.nupkg`);

    console.log(`[build-windows] Downloading Python Windows NuGet package (${version}) from NuGet...`);

    downloadFile(nugetUrl, tmpFilePath, (err) => {
        if (err) {
            console.error('[build-windows] Failed to download Windows python libs:', err.message);
            process.exit(1);
        }
        console.log('[build-windows] Download complete. Extracting NuGet libs...');

        try {
            if (!fs.existsSync(libsDir)) {
                fs.mkdirSync(libsDir, { recursive: true });
            }

            execSync(`unzip -q -o "${tmpFilePath}" "tools/libs/*" -d "${libsDir}"`);

            const toolsLibsPath = path.join(libsDir, 'tools', 'libs');
            if (fs.existsSync(toolsLibsPath)) {
                fs.renameSync(toolsLibsPath, pythonWindowsDir);
                fs.rmdirSync(path.join(libsDir, 'tools'));
            }

            fs.unlinkSync(tmpFilePath);
            console.log('[build-windows] Python Windows libraries set up successfully.');
            triggerBuild();
        } catch (e) {
            console.error('[build-windows] Failed to setup Windows python libs after download:', e.message);
            process.exit(1);
        }
    });
}
