
const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

const ndkHome = process.env.NDK_HOME || path.join(os.homedir(), "AppData/Local/Android/Sdk/ndk/27.0.12077973");
const toolchainBin = path.join(ndkHome, "toolchains/llvm/prebuilt/windows-x86_64/bin");

if (!fs.existsSync(toolchainBin)) {
    console.error("NDK not found at " + toolchainBin);
    process.exit(1);
}

const env = { ...process.env };
env.CC_aarch64_linux_android = path.join(toolchainBin, "aarch64-linux-android24-clang.cmd");
env.CXX_aarch64_linux_android = path.join(toolchainBin, "aarch64-linux-android24-clang++.cmd");
env.CC_armv7_linux_androideabi = path.join(toolchainBin, "armv7a-linux-androideabi24-clang.cmd");
env.CXX_armv7_linux_androideabi = path.join(toolchainBin, "armv7a-linux-androideabi24-clang++.cmd");
env.CC_i686_linux_android = path.join(toolchainBin, "i686-linux-android24-clang.cmd");
env.CXX_i686_linux_android = path.join(toolchainBin, "i686-linux-android24-clang++.cmd");
env.CC_x86_64_linux_android = path.join(toolchainBin, "x86_64-linux-android24-clang.cmd");
env.CXX_x86_64_linux_android = path.join(toolchainBin, "x86_64-linux-android24-clang++.cmd");

const result = spawnSync("npx", ["tauri", "android", "build"], {
    env,
    stdio: "inherit",
    shell: true
});
process.exit(result.status);

