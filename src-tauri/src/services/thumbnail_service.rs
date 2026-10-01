use std::{
    collections::HashSet,
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{
    ColorType, GenericImageView, ImageEncoder, ImageReader, Limits, codecs::png::PngEncoder,
};
use serde::Serialize;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::{
    domain::{MediaType, PathKind, StoredPathKind},
    services::{WorkspaceService, WriteAccessGuard},
};

const THUMBNAIL_EDGE: u32 = 512;
const MAX_SOURCE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_SOURCE_PIXELS: u64 = 40_000_000;
const MAX_DECODE_BYTES: u64 = 160 * 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 8 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 32;
const WORKER_COUNT: usize = 2;
const CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const CACHE_TARGET_BYTES: u64 = 448 * 1024 * 1024;
const CACHE_RELATIVE_PATH: &str = "cache/thumbnails";
const CACHE_VERSION: &str = "v1";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFinalPathNameByHandleW(
        file: *mut c_void,
        path: *mut u16,
        path_capacity: u32,
        flags: u32,
    ) -> u32;
}

/// 缩略图请求只保存资产已存储的媒体描述，绝不接收前端传入的路径。
#[derive(Debug, Clone)]
pub(crate) struct ThumbnailSource {
    pub(crate) workspace_root: PathBuf,
    pub(crate) asset_id: i64,
    pub(crate) media_type: MediaType,
    pub(crate) path_kind: PathKind,
    pub(crate) stored_path: String,
    pub(crate) content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ThumbnailResponse {
    Ready {
        bytes: Vec<u8>,
        mime_type: &'static str,
    },
    Pending,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ThumbnailKey {
    workspace_root: PathBuf,
    asset_id: i64,
    source_hash: String,
}

struct ThumbnailJob {
    source: ThumbnailSource,
    key: ThumbnailKey,
}

/// 进程级缩略图队列。队列与线程数固定，避免滚动列表引入无界任务或线程。
#[derive(Clone)]
pub(crate) struct ThumbnailService {
    sender: SyncSender<ThumbnailJob>,
    pending: Arc<Mutex<HashSet<ThumbnailKey>>>,
}

impl Default for ThumbnailService {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<ThumbnailJob>(QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        let pending = Arc::new(Mutex::new(HashSet::new()));

        for worker_index in 0..WORKER_COUNT {
            let receiver = Arc::clone(&receiver);
            let pending = Arc::clone(&pending);
            let _ = thread::Builder::new()
                .name(format!("thumbnail-worker-{worker_index}"))
                .spawn(move || {
                    loop {
                        let job = match receiver.lock().ok().and_then(|guard| guard.recv().ok()) {
                            Some(job) => job,
                            None => return,
                        };
                        let _ = generate_thumbnail(&job.source, &job.key);
                        if let Ok(mut jobs) = pending.lock() {
                            jobs.remove(&job.key);
                        }
                    }
                });
        }

        Self { sender, pending }
    }
}

impl ThumbnailService {
    /// 命中缓存立即返回；未命中只入队，调用方可安全重试，不会同步解码原图。
    pub(crate) fn read_or_enqueue(&self, source: ThumbnailSource) -> ThumbnailResponse {
        if source.media_type != MediaType::Image || source.asset_id <= 0 {
            return ThumbnailResponse::Unavailable;
        }

        let (source_file, source_metadata, cache_root) = match open_source(&source) {
            Ok(value) => value,
            Err(()) => return ThumbnailResponse::Unavailable,
        };
        drop(source_file);
        let key = thumbnail_key(&source, &source_metadata);
        match read_cached_thumbnail(&cache_root, &key, &source_metadata) {
            Ok(Some(bytes)) => {
                return ThumbnailResponse::Ready {
                    bytes,
                    mime_type: "image/png",
                };
            }
            Ok(None) => {}
            Err(()) => return ThumbnailResponse::Unavailable,
        }

        // 只读展示模式仍可读取既有缓存，但不得为缺失项排队写入新缩略图。
        if WriteAccessGuard::ensure_writable(&source.workspace_root).is_err() {
            return ThumbnailResponse::Unavailable;
        }

        let mut pending = match self.pending.lock() {
            Ok(value) => value,
            Err(_) => return ThumbnailResponse::Unavailable,
        };
        if pending.contains(&key) {
            return ThumbnailResponse::Pending;
        }
        match self.sender.try_send(ThumbnailJob {
            source,
            key: key.clone(),
        }) {
            Ok(()) => {
                pending.insert(key);
                ThumbnailResponse::Pending
            }
            Err(TrySendError::Full(_)) => ThumbnailResponse::Pending,
            Err(TrySendError::Disconnected(_)) => ThumbnailResponse::Unavailable,
        }
    }
}

fn generate_thumbnail(source: &ThumbnailSource, expected_key: &ThumbnailKey) -> Result<(), ()> {
    WriteAccessGuard::ensure_writable(&source.workspace_root).map_err(|_| ())?;
    let (file, metadata, cache_root) = open_source(source)?;
    let key = thumbnail_key(source, &metadata);
    if &key != expected_key {
        return Err(());
    }
    if read_cached_thumbnail(&cache_root, &key, &metadata)?.is_some() {
        return Ok(());
    }

    let mut reader = ImageReader::new(BufReader::new(file));
    reader = reader.with_guessed_format().map_err(|_| ())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| ())?;
    let (width, height) = image.dimensions();
    let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(())?;
    if pixels == 0 || pixels > MAX_SOURCE_PIXELS {
        return Err(());
    }

    let thumbnail = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE).to_rgba8();
    let (width, height) = thumbnail.dimensions();
    let cache_path = cache_path(&cache_root, &key);
    // 写租约覆盖缩略图落盘与缓存裁剪；只读切换会等待这段写入完成。
    let _lease = WriteAccessGuard::acquire_writable(&source.workspace_root).map_err(|_| ())?;
    write_thumbnail_atomically(&cache_root, &cache_path, &thumbnail, width, height)?;
    prune_cache(&cache_root, CACHE_MAX_BYTES, CACHE_TARGET_BYTES);
    Ok(())
}

fn open_source(source: &ThumbnailSource) -> Result<(File, fs::Metadata, PathBuf), ()> {
    let root = &source.workspace_root;
    let stored_path = Path::new(&source.stored_path);
    let kind = match source.path_kind {
        PathKind::Managed => StoredPathKind::Managed,
        PathKind::External => StoredPathKind::External,
    };
    WorkspaceService::new()
        .check_stored_path(root, kind, stored_path)
        .map_err(|_| ())?;

    let path = match source.path_kind {
        PathKind::Managed => root.join(stored_path),
        PathKind::External => stored_path.to_path_buf(),
    };
    let trusted_media_root = matches!(source.path_kind, PathKind::Managed)
        .then(|| root.join("media").canonicalize())
        .transpose()
        .map_err(|_| ())?;
    let cache_root = trusted_cache_root(root)?;
    let file = File::open(path).map_err(|_| ())?;
    if let Some(media_root) = trusted_media_root.as_deref() {
        ensure_handle_inside_root(&file, media_root)?;
    }
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return Err(());
    }
    Ok((file, metadata, cache_root))
}

fn trusted_cache_root(workspace_root: &Path) -> Result<PathBuf, ()> {
    let workspace_root = workspace_root.canonicalize().map_err(|_| ())?;
    let cache_root = workspace_root
        .join(CACHE_RELATIVE_PATH)
        .canonicalize()
        .map_err(|_| ())?;
    if !cache_root.starts_with(&workspace_root) {
        return Err(());
    }
    let metadata = fs::symlink_metadata(&cache_root).map_err(|_| ())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(());
    }
    Ok(cache_root)
}

fn thumbnail_key(source: &ThumbnailSource, metadata: &fs::Metadata) -> ThumbnailKey {
    let hash = source
        .content_hash
        .as_deref()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map_or_else(|| "unhashed".to_owned(), |value| value[..32].to_owned());
    // 文件长度与修改时间进入键，引用媒体被替换后不会继续命中旧缩略图。
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    ThumbnailKey {
        workspace_root: source.workspace_root.clone(),
        asset_id: source.asset_id,
        source_hash: format!("{hash}-{}-{modified}", metadata.len()),
    }
}

fn cache_path(cache_root: &Path, key: &ThumbnailKey) -> PathBuf {
    cache_root.join(format!(
        "{CACHE_VERSION}-{}-{}.png",
        key.asset_id, key.source_hash
    ))
}

fn read_cached_thumbnail(
    cache_root: &Path,
    key: &ThumbnailKey,
    source_metadata: &fs::Metadata,
) -> Result<Option<Vec<u8>>, ()> {
    let path = cache_path(cache_root, key);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_THUMBNAIL_BYTES
    {
        return Ok(None);
    }
    let file = File::open(&path).map_err(|_| ())?;
    ensure_handle_inside_root(&file, cache_root)?;
    let cached_metadata = file.metadata().map_err(|_| ())?;
    if !cached_metadata.is_file() || cached_metadata.len() > MAX_THUMBNAIL_BYTES {
        return Ok(None);
    }
    if !cache_is_fresh(&cached_metadata, source_metadata) {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(cached_metadata.len() as usize);
    file.take(MAX_THUMBNAIL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_THUMBNAIL_BYTES {
        return Ok(None);
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(None);
    }
    touch(&path);
    Ok(Some(bytes))
}

fn cache_is_fresh(cache: &fs::Metadata, source: &fs::Metadata) -> bool {
    match (cache.modified(), source.modified()) {
        (Ok(cache_time), Ok(source_time)) => cache_time >= source_time,
        _ => false,
    }
}

fn write_thumbnail_atomically(
    cache_root: &Path,
    cache_path: &Path,
    image: &image::RgbaImage,
    width: u32,
    height: u32,
) -> Result<(), ()> {
    let file_name = cache_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(())?;
    let temporary = cache_root.join(format!(
        ".{file_name}-{}-{}.tmp",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| ())?;
        ensure_handle_inside_root(&file, cache_root)?;
        PngEncoder::new(&mut file)
            .write_image(image.as_raw(), width, height, ColorType::Rgba8.into())
            .map_err(|_| ())?;
        file.flush().map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        if trusted_cache_root_for_existing_path(cache_root)? != cache_root {
            return Err(());
        }
        match fs::rename(&temporary, cache_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temporary);
                Ok(())
            }
            Err(_) => Err(()),
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn trusted_cache_root_for_existing_path(cache_root: &Path) -> Result<PathBuf, ()> {
    let verified = cache_root.canonicalize().map_err(|_| ())?;
    let metadata = fs::symlink_metadata(&verified).map_err(|_| ())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(());
    }
    Ok(verified)
}

fn prune_cache(cache_root: &Path, maximum: u64, target: u64) {
    let mut entries = Vec::new();
    let mut total = 0_u64;
    let Ok(read_dir) = fs::read_dir(cache_root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !is_cache_thumbnail_name(&path) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(cache_root) {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        total = total.saturating_add(metadata.len());
        entries.push((modified, path, metadata.len()));
    }
    if total <= maximum {
        return;
    }
    entries.sort_by_key(|(modified, _, _)| *modified);
    for (_, path, size) in entries {
        if total <= target {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

fn is_cache_thumbnail_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.starts_with(&format!("{CACHE_VERSION}-")) && name.ends_with(".png")
        })
}

fn touch(path: &Path) {
    if let Ok(file) = OpenOptions::new().write(true).open(path) {
        let _ = file.set_times(fs::FileTimes::new().set_modified(SystemTime::now()));
    }
}

#[cfg(windows)]
pub(crate) fn ensure_handle_inside_root(file: &File, trusted_root: &Path) -> Result<(), ()> {
    let mut buffer = vec![0_u16; 32_768];
    // 校验已打开句柄的最终路径，避免预检后目录链接或文件被替换。
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        return Err(());
    }
    let opened = String::from_utf16(&buffer[..length as usize]).map_err(|_| ())?;
    let opened = normalize_windows_final_path(&opened);
    let root = normalize_windows_final_path(&trusted_root.to_string_lossy());
    (opened == root || opened.starts_with(&format!("{root}\\")))
        .then_some(())
        .ok_or(())
}

#[cfg(windows)]
fn normalize_windows_final_path(value: &str) -> String {
    let normalized = value
        .strip_prefix(r"\\?\UNC\")
        .map(|path| format!(r"\\{path}"))
        .or_else(|| value.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| value.to_owned());
    normalized.trim_end_matches(['\\', '/']).to_lowercase()
}

#[cfg(not(windows))]
pub(crate) fn ensure_handle_inside_root(_file: &File, trusted_root: &Path) -> Result<(), ()> {
    trusted_root.is_absolute().then_some(()).ok_or(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, SystemTime},
    };

    use image::{DynamicImage, GenericImageView, Rgba};

    use super::{
        CACHE_VERSION, QUEUE_CAPACITY, ThumbnailResponse, ThumbnailService, ThumbnailSource,
        WORKER_COUNT, prune_cache,
    };
    use crate::{
        domain::{AccessMode, MediaType, PathKind},
        services::{AccessModeService, DatabaseService, WorkspaceService},
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-thumbnail-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("应能创建测试父目录");
            Self(path)
        }

        fn workspace(&self) -> PathBuf {
            self.0.join("workspace")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn source(root: &Path, media_type: MediaType, stored_path: &str) -> ThumbnailSource {
        ThumbnailSource {
            workspace_root: root.to_path_buf(),
            asset_id: 7,
            media_type,
            path_kind: PathKind::Managed,
            stored_path: stored_path.to_owned(),
            content_hash: Some("a".repeat(64)),
        }
    }

    fn wait_ready(service: &ThumbnailService, source: ThumbnailSource) -> Vec<u8> {
        for _ in 0..100 {
            match service.read_or_enqueue(source.clone()) {
                ThumbnailResponse::Ready { bytes, .. } => return bytes,
                ThumbnailResponse::Pending => thread::sleep(Duration::from_millis(10)),
                ThumbnailResponse::Unavailable => panic!("有效图片不应不可用"),
            }
        }
        panic!("缩略图未在期限内生成");
    }

    #[test]
    fn images_are_generated_on_demand_and_then_hit_cache() {
        let parent = TestDirectory::new();
        let root = parent.workspace();
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建测试工作区");
        let image_path = root.join("media/images/example.png");
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1_024,
            256,
            Rgba([1, 2, 3, 255]),
        ))
        .save(&image_path)
        .expect("应能写入测试图片");
        let service = ThumbnailService::default();
        let request = source(&root, MediaType::Image, "media/images/example.png");

        assert_eq!(
            service.read_or_enqueue(request.clone()),
            ThumbnailResponse::Pending
        );
        let bytes = wait_ready(&service, request.clone());
        let image = image::load_from_memory(&bytes).expect("缓存必须是可读 PNG");
        assert_eq!(image.dimensions(), (512, 128));
        assert!(matches!(
            service.read_or_enqueue(request),
            ThumbnailResponse::Ready { .. }
        ));
    }

    #[test]
    fn ready_response_uses_frontend_camel_case_fields() {
        let response = ThumbnailResponse::Ready {
            bytes: vec![1, 2, 3],
            mime_type: "image/png",
        };
        let json = serde_json::to_value(response).expect("缩略图响应应可序列化");
        assert_eq!(json["state"], "ready");
        assert_eq!(json["mimeType"], "image/png");
        assert!(json.get("mime_type").is_none(), "不得泄漏 snake_case 字段");
    }

    #[test]
    fn videos_and_unsafe_paths_are_unavailable() {
        let parent = TestDirectory::new();
        let root = parent.workspace();
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建测试工作区");
        let service = ThumbnailService::default();
        assert_eq!(
            service.read_or_enqueue(source(&root, MediaType::Video, "media/videos/example.mp4")),
            ThumbnailResponse::Unavailable
        );
        assert_eq!(
            service.read_or_enqueue(source(&root, MediaType::Image, "../outside.png")),
            ThumbnailResponse::Unavailable
        );
    }

    #[test]
    fn read_only_workspace_does_not_enqueue_or_write_missing_thumbnail() {
        let parent = TestDirectory::new();
        let root = parent.workspace();
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建测试工作区");
        DatabaseService::new()
            .prepare_workspace_database(&root)
            .expect("应能准备当前数据库");
        let image_path = root.join("media/images/read-only.png");
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(16, 16, Rgba([1, 2, 3, 255])))
            .save(&image_path)
            .expect("应能写入测试图片");
        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能设为只读");

        let response = ThumbnailService::default().read_or_enqueue(source(
            &root,
            MediaType::Image,
            "media/images/read-only.png",
        ));

        assert_eq!(response, ThumbnailResponse::Unavailable);
        let cache_entries = fs::read_dir(root.join("cache/thumbnails"))
            .expect("缩略图缓存目录应存在")
            .count();
        assert_eq!(cache_entries, 0, "只读模式不得创建或排队生成缩略图");
    }

    #[test]
    fn lru_pruning_only_removes_cache_png_files() {
        let parent = TestDirectory::new();
        let root = parent.workspace();
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建测试工作区");
        let cache = root.join("cache/thumbnails");
        for index in 0..3 {
            let path = cache.join(format!("{CACHE_VERSION}-{index}-hash.png"));
            fs::write(&path, vec![index as u8; 6]).expect("应能写入缓存文件");
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(index);
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .expect("应能打开缓存文件")
                .set_times(fs::FileTimes::new().set_modified(time))
                .expect("应能设置 LRU 时间");
        }
        fs::write(cache.join("keep.txt"), b"keep").expect("应能写入非缓存文件");

        prune_cache(&cache.canonicalize().expect("缓存目录应存在"), 12, 6);

        assert!(!cache.join(format!("{CACHE_VERSION}-0-hash.png")).exists());
        assert!(cache.join("keep.txt").exists());
    }

    #[test]
    fn queue_limits_are_fixed_for_all_thumbnail_services() {
        assert_eq!(QUEUE_CAPACITY, 32);
        assert_eq!(WORKER_COUNT, 2);
    }
}
