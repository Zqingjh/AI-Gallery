import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  renameSync,
  rmSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(scriptDirectory, "..");
const binaryDirectory = join(projectRoot, "src-tauri", "binaries");
const binaryPath = join(binaryDirectory, "ffmpeg.exe");
const temporaryDirectory = join(binaryDirectory, ".ffmpeg-download");
const archivePath = join(temporaryDirectory, "ffmpeg.zip");
const extractDirectory = join(temporaryDirectory, "extracted");

const source = {
  version: "7.1.5-1-g7d0e842004",
  url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-06-30-13-34/ffmpeg-n7.1.5-1-g7d0e842004-win64-lgpl-7.1.zip",
  archiveSha256:
    "ec1c6ae03fab10f316344973f83c549b4b662ec3d73f1658353ab1587f4cf727",
  executableSha256:
    "e46b5b83f8a5ff3790cc27ad1d8947146c0a22661f0337f701769f6d75bf998a",
};

if (process.platform !== "win32") {
  throw new Error("内置视频封面组件当前仅支持 Windows x64 构建。");
}

if (existsSync(binaryPath)) {
  if ((await sha256(binaryPath)) === source.executableSha256) {
    rmSync(temporaryDirectory, { recursive: true, force: true });
    process.stdout.write("已找到并校验内置 FFmpeg。\n");
    process.exit(0);
  }
  rmSync(binaryPath, { force: true });
}

mkdirSync(binaryDirectory, { recursive: true });
mkdirSync(temporaryDirectory, { recursive: true });
rmSync(extractDirectory, { recursive: true, force: true });
mkdirSync(extractDirectory, { recursive: true });

try {
  process.stdout.write(`正在下载并校验 FFmpeg ${source.version}…\n`);
  execFileSync(
    "curl.exe",
    [
      "--fail",
      "--location",
      "--continue-at",
      "-",
      "--retry",
      "3",
      "--silent",
      "--show-error",
      "--output",
      archivePath,
      source.url,
    ],
    { stdio: "inherit" },
  );
  const archiveHash = await sha256(archivePath);
  if (archiveHash !== source.archiveSha256) {
    throw new Error("内置 FFmpeg 下载校验失败，已拒绝使用该文件。");
  }

  execFileSync("tar", ["-xf", archivePath, "-C", extractDirectory], {
    stdio: "ignore",
  });
  const extractedBinary = findFile(extractDirectory, "ffmpeg.exe");
  if (!extractedBinary) {
    throw new Error("FFmpeg 压缩包中未找到可执行文件。");
  }
  const temporaryBinary = join(binaryDirectory, "ffmpeg.exe.tmp");
  cpSync(extractedBinary, temporaryBinary);
  renameSync(temporaryBinary, binaryPath);
  process.stdout.write("内置 FFmpeg 已就绪。\n");
} finally {
  rmSync(temporaryDirectory, { recursive: true, force: true });
}

async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

function findFile(directory, expectedName) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      const nested = findFile(path, expectedName);
      if (nested) return nested;
    } else if (entry.isFile() && entry.name.toLowerCase() === expectedName) {
      return path;
    }
  }
  return undefined;
}
