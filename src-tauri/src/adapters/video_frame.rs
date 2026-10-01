use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use image::{ImageFormat, ImageReader};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_PNG_BYTES: u64 = 10 * 1024 * 1024;
const MAX_PNG_EDGE: u32 = 8192;
const MAX_PNG_PIXELS: u64 = 40_000_000;

#[derive(Debug, Clone)]
pub(crate) struct VideoFrameRequest {
    pub(crate) ffmpeg_executable: PathBuf,
    pub(crate) input_path: PathBuf,
    pub(crate) output_path: PathBuf,
    pub(crate) timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoFrameError {
    InvalidExecutable,
    InvalidInput,
    InvalidOutput,
    OutputAlreadyExists,
    SpawnFailed,
    Cancelled,
    TimedOut,
    ProcessFailed,
    InvalidPng,
    OutputTooLarge,
    CleanupFailed,
}

impl std::fmt::Display for VideoFrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidExecutable => "FFmpeg 可执行文件无效",
            Self::InvalidInput => "视频输入文件无效",
            Self::InvalidOutput => "关键帧输出位置无效",
            Self::OutputAlreadyExists => "关键帧输出文件已存在",
            Self::SpawnFailed => "无法启动关键帧生成组件",
            Self::Cancelled => "关键帧生成已取消",
            Self::TimedOut => "关键帧生成超时",
            Self::ProcessFailed => "关键帧生成失败",
            Self::InvalidPng => "关键帧不是有效的 PNG 图片",
            Self::OutputTooLarge => "关键帧超出安全大小限制",
            Self::CleanupFailed => "关键帧失败产物清理失败",
        })
    }
}

impl std::error::Error for VideoFrameError {}

#[derive(Debug, Clone, Default)]
pub(crate) struct VideoFrameCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl VideoFrameCancellationToken {
    #[cfg(test)]
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// 只运行用户显式选择的 FFmpeg，不搜索 PATH，也不通过 shell 拼接命令。
pub(crate) fn generate_video_frame(
    request: &VideoFrameRequest,
    cancellation: &VideoFrameCancellationToken,
) -> Result<(), VideoFrameError> {
    generate_video_frame_with_timeout(request, cancellation, PROCESS_TIMEOUT)
}

fn generate_video_frame_with_timeout(
    request: &VideoFrameRequest,
    cancellation: &VideoFrameCancellationToken,
    timeout: Duration,
) -> Result<(), VideoFrameError> {
    let (ffmpeg_executable, input_path) = validate_paths(request)?;
    if cancellation.is_cancelled() {
        return Err(VideoFrameError::Cancelled);
    }

    let timestamp = format!(
        "{}.{:03}",
        request.timestamp_ms / 1000,
        request.timestamp_ms % 1000
    );
    let mut command = Command::new(&ffmpeg_executable);
    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss"])
        .arg(timestamp)
        .arg("-i")
        .arg(&input_path)
        .args([
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-dn",
            "-f",
            "image2",
            "-vcodec",
            "png",
            "-n",
        ])
        .arg(&request.output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn().map_err(|_| VideoFrameError::SpawnFailed)?;
    let started_at = Instant::now();
    loop {
        if cancellation.is_cancelled() {
            terminate_and_reap(&mut child);
            cleanup_generated_output(&request.output_path)?;
            return Err(VideoFrameError::Cancelled);
        }
        if started_at.elapsed() >= timeout {
            terminate_and_reap(&mut child);
            cleanup_generated_output(&request.output_path)?;
            return Err(VideoFrameError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                cleanup_generated_output(&request.output_path)?;
                return Err(VideoFrameError::ProcessFailed);
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(_) => {
                terminate_and_reap(&mut child);
                cleanup_generated_output(&request.output_path)?;
                return Err(VideoFrameError::ProcessFailed);
            }
        }
    }

    if let Err(error) = validate_png(&request.output_path) {
        cleanup_generated_output(&request.output_path)?;
        return Err(error);
    }
    Ok(())
}

fn validate_paths(request: &VideoFrameRequest) -> Result<(PathBuf, PathBuf), VideoFrameError> {
    if !request.ffmpeg_executable.is_absolute() {
        return Err(VideoFrameError::InvalidExecutable);
    }
    let executable_metadata = fs::symlink_metadata(&request.ffmpeg_executable)
        .map_err(|_| VideoFrameError::InvalidExecutable)?;
    if !executable_metadata.is_file() || is_link_or_reparse(&executable_metadata) {
        return Err(VideoFrameError::InvalidExecutable);
    }
    if !request.input_path.is_absolute() {
        return Err(VideoFrameError::InvalidInput);
    }
    let input_metadata =
        fs::symlink_metadata(&request.input_path).map_err(|_| VideoFrameError::InvalidInput)?;
    if !input_metadata.is_file() || is_link_or_reparse(&input_metadata) {
        return Err(VideoFrameError::InvalidInput);
    }
    if !request.output_path.is_absolute()
        || request.output_path.file_name().is_none()
        || !request
            .output_path
            .parent()
            .is_some_and(|parent| parent.is_dir())
    {
        return Err(VideoFrameError::InvalidOutput);
    }
    if request.output_path.exists() {
        return Err(VideoFrameError::OutputAlreadyExists);
    }
    let executable = request
        .ffmpeg_executable
        .canonicalize()
        .map_err(|_| VideoFrameError::InvalidExecutable)?;
    let input = request
        .input_path
        .canonicalize()
        .map_err(|_| VideoFrameError::InvalidInput)?;
    Ok((executable, input))
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn terminate_and_reap(child: &mut std::process::Child) {
    // kill 失败时仍调用 wait，避免子进程变成僵尸进程。
    let _ = child.kill();
    let _ = child.wait();
}

fn cleanup_generated_output(path: &Path) -> Result<(), VideoFrameError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(VideoFrameError::CleanupFailed),
    }
}

fn validate_png(path: &Path) -> Result<(), VideoFrameError> {
    let metadata = fs::metadata(path).map_err(|_| VideoFrameError::InvalidPng)?;
    if metadata.len() == 0 {
        return Err(VideoFrameError::InvalidPng);
    }
    if metadata.len() > MAX_PNG_BYTES {
        return Err(VideoFrameError::OutputTooLarge);
    }
    let mut file = File::open(path).map_err(|_| VideoFrameError::InvalidPng)?;
    let mut signature = [0_u8; 8];
    file.read_exact(&mut signature)
        .map_err(|_| VideoFrameError::InvalidPng)?;
    if signature != *b"\x89PNG\r\n\x1a\n" {
        return Err(VideoFrameError::InvalidPng);
    }
    drop(file);
    let file = File::open(path).map_err(|_| VideoFrameError::InvalidPng)?;
    let reader = ImageReader::with_format(BufReader::new(file), ImageFormat::Png);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| VideoFrameError::InvalidPng)?;
    if width == 0
        || height == 0
        || width > MAX_PNG_EDGE
        || height > MAX_PNG_EDGE
        || u64::from(width) * u64::from(height) > MAX_PNG_PIXELS
    {
        return Err(VideoFrameError::OutputTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use image::{DynamicImage, Rgba, RgbaImage};

    use super::{
        VideoFrameCancellationToken, VideoFrameError, VideoFrameRequest,
        generate_video_frame_with_timeout, validate_png,
    };

    static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ai-gallery-video-frame-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("应能创建测试目录");
        root
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([1, 2, 3, 255])))
            .save_with_format(path, image::ImageFormat::Png)
            .expect("应能写入 PNG 测试文件");
    }

    #[test]
    fn png_validation_accepts_bounded_image_and_rejects_fake_png() {
        let root = temp_root("png");
        let valid = root.join("valid.png");
        write_png(&valid, 8, 8);
        assert_eq!(validate_png(&valid), Ok(()));

        let invalid = root.join("invalid.png");
        fs::write(&invalid, b"not a png").expect("应能写入无效文件");
        assert_eq!(validate_png(&invalid), Err(VideoFrameError::InvalidPng));
        fs::remove_dir_all(root).expect("应能清理测试目录");
    }

    #[test]
    fn validation_requires_explicit_absolute_executable() {
        let root = temp_root("paths");
        let input = root.join("input.mp4");
        fs::write(&input, b"video").expect("应能写入测试视频");
        let request = VideoFrameRequest {
            ffmpeg_executable: PathBuf::from("ffmpeg"),
            input_path: input,
            output_path: root.join("cover.png"),
            timestamp_ms: 1000,
        };
        let error = generate_video_frame_with_timeout(
            &request,
            &VideoFrameCancellationToken::default(),
            Duration::from_millis(50),
        )
        .expect_err("相对 FFmpeg 路径必须拒绝");
        assert_eq!(error, VideoFrameError::InvalidExecutable);
        assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
        fs::remove_dir_all(root).expect("应能清理测试目录");
    }

    #[test]
    fn pre_cancelled_task_does_not_start_process() {
        let root = temp_root("cancel");
        let executable = root.join("fake-ffmpeg.exe");
        let input = root.join("input.mp4");
        fs::write(&executable, b"not executable").expect("应能写入占位程序");
        fs::write(&input, b"video").expect("应能写入测试视频");
        let cancellation = VideoFrameCancellationToken::default();
        cancellation.cancel();
        let request = VideoFrameRequest {
            ffmpeg_executable: executable,
            input_path: input,
            output_path: root.join("cover.png"),
            timestamp_ms: 0,
        };
        assert_eq!(
            generate_video_frame_with_timeout(&request, &cancellation, Duration::from_millis(50)),
            Err(VideoFrameError::Cancelled)
        );
        fs::remove_dir_all(root).expect("应能清理测试目录");
    }
}
