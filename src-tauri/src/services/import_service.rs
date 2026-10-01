use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::UNIX_EPOCH,
};

use crate::{
    adapters::media_metadata::{BasicMediaMetadataAdapter, MediaMetadataAdapter},
    domain::{ImportCancellationToken, ImportMode, MediaKind, PreparedMediaImport},
};

use super::{AccessModeServiceError, WorkspaceService, WriteAccessGuard, WriteAccessLease};

// 覆盖 P0 的 100 张图片 + 20 个视频，同时给调用层保留合理分批余量。
const MAX_BATCH_SIZE: usize = 256;
const MAX_IMPORT_WORKERS: usize = 2;
const STREAM_BUFFER_SIZE: usize = 64 * 1024;
const MAX_DESTINATION_ATTEMPTS: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportServiceError {
    InvalidWorkspace,
    EmptyBatch,
    BatchTooLarge,
    InvalidSource,
    UnsupportedFormat,
    SourceUnavailable,
    SourceChanged,
    ReadFailed,
    CopyFailed,
    Cancelled,
    CompensationFailed,
    ReadOnly,
}

pub(crate) struct PrepareImportBatchRequest {
    pub(crate) workspace_root: PathBuf,
    pub(crate) import_mode: ImportMode,
    pub(crate) source_paths: Vec<PathBuf>,
}

pub(crate) struct PreparedImportBatch {
    workspace_root: PathBuf,
    items: Vec<PreparedMediaImport>,
    copied_paths: Vec<PathBuf>,
}

impl std::fmt::Debug for PreparedImportBatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedImportBatch")
            .field("workspace_root", &"<redacted>")
            .field("items", &self.items)
            .field("copied_count", &self.copied_paths.len())
            .finish()
    }
}

impl PreparedImportBatch {
    pub(crate) fn items(&self) -> &[PreparedMediaImport] {
        &self.items
    }
}

pub(crate) struct ImportService {
    metadata_adapter: Arc<dyn MediaMetadataAdapter>,
}

impl Default for ImportService {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportService {
    pub(crate) fn new() -> Self {
        Self {
            metadata_adapter: Arc::new(BasicMediaMetadataAdapter),
        }
    }

    #[cfg(test)]
    fn with_metadata_adapter(metadata_adapter: Arc<dyn MediaMetadataAdapter>) -> Self {
        Self { metadata_adapter }
    }

    #[cfg(test)]
    pub(crate) fn prepare_batch(
        &self,
        request: PrepareImportBatchRequest,
        cancellation: &ImportCancellationToken,
    ) -> Result<PreparedImportBatch, ImportServiceError> {
        self.prepare_batch_with_progress(request, cancellation, &|_, _| {})
    }

    pub(crate) fn prepare_batch_with_progress(
        &self,
        request: PrepareImportBatchRequest,
        cancellation: &ImportCancellationToken,
        on_progress: &(dyn Fn(usize, &str) + Sync),
    ) -> Result<PreparedImportBatch, ImportServiceError> {
        WorkspaceService::new()
            .open_workspace(&request.workspace_root)
            .map_err(|_| ImportServiceError::InvalidWorkspace)?;
        ensure_writable(&request.workspace_root)?;
        if request.source_paths.is_empty() {
            return Err(ImportServiceError::EmptyBatch);
        }
        if request.source_paths.len() > MAX_BATCH_SIZE {
            return Err(ImportServiceError::BatchTooLarge);
        }
        if cancellation.is_cancelled() {
            return Err(ImportServiceError::Cancelled);
        }

        let mut work = VecDeque::with_capacity(request.source_paths.len());
        for (index, source_path) in request.source_paths.into_iter().enumerate() {
            let media_format = inspect_source(&source_path)?;
            work.push_back((index, source_path, media_format));
        }

        let item_count = work.len();
        let queue = Mutex::new(work);
        let results = Mutex::new((0..item_count).map(|_| None).collect::<Vec<_>>());
        let stop = AtomicBool::new(false);
        let completed = AtomicUsize::new(0);
        thread::scope(|scope| {
            for _ in 0..item_count.min(MAX_IMPORT_WORKERS) {
                scope.spawn(|| {
                    loop {
                        if stop.load(Ordering::Acquire) || cancellation.is_cancelled() {
                            break;
                        }
                        let next = queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .pop_front();
                        let Some((index, source_path, media_format)) = next else {
                            break;
                        };
                        let result = self.prepare_one(
                            index,
                            &request.workspace_root,
                            request.import_mode,
                            &source_path,
                            media_format,
                            cancellation,
                        );
                        if result.is_err() {
                            stop.store(true, Ordering::Release);
                        } else if let Ok(prepared) = &result {
                            let completed = completed.fetch_add(1, Ordering::AcqRel) + 1;
                            on_progress(completed, &prepared.item.file_name);
                        }
                        results
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())[index] = Some(result);
                    }
                });
            }
        });

        let mut prepared_items = Vec::with_capacity(item_count);
        let mut copied_paths = Vec::new();
        let mut first_error = None;
        for result in results
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .into_iter()
            .flatten()
        {
            match result {
                Ok(prepared) => {
                    if let Some(path) = prepared.copied_path {
                        copied_paths.push(path);
                    }
                    prepared_items.push((prepared.index, prepared.item));
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            };
        }

        if cancellation.is_cancelled()
            || first_error.is_some()
            || prepared_items.len() != item_count
        {
            remove_copied_files(&copied_paths)?;
            return Err(if cancellation.is_cancelled() {
                ImportServiceError::Cancelled
            } else {
                first_error.unwrap_or(ImportServiceError::ReadFailed)
            });
        }

        prepared_items.sort_unstable_by_key(|(index, _)| *index);
        Ok(PreparedImportBatch {
            workspace_root: request.workspace_root,
            items: prepared_items.into_iter().map(|(_, item)| item).collect(),
            copied_paths,
        })
    }

    pub(crate) fn compensate_batch(
        &self,
        batch: &PreparedImportBatch,
    ) -> Result<(), ImportServiceError> {
        // cleanup 句柄只包含本服务使用 create_new 创建的路径；额外边界检查防止未来误构造。
        let managed_root = batch.workspace_root.join("media");
        if batch
            .copied_paths
            .iter()
            .any(|path| !path.starts_with(&managed_root))
        {
            return Err(ImportServiceError::CompensationFailed);
        }
        remove_copied_files(&batch.copied_paths)
    }

    fn prepare_one(
        &self,
        index: usize,
        workspace_root: &Path,
        import_mode: ImportMode,
        source_path: &Path,
        media_format: MediaFormat,
        cancellation: &ImportCancellationToken,
    ) -> Result<PreparedWorkerResult, ImportServiceError> {
        let file_name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(ImportServiceError::InvalidSource)?
            .to_owned();
        let content_hash = hash_file(source_path, cancellation)?;
        let source_modified_at = fs::metadata(source_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_millis()).ok());
        let (stored_path, metadata_path, copied_path) = match import_mode {
            ImportMode::Reference => (source_path.to_path_buf(), source_path.to_path_buf(), None),
            ImportMode::Copy => {
                // 覆盖目标文件创建、写入与同步整个区间，切换只读会先等待本次复制结束。
                let _lease = acquire_writable(workspace_root)?;
                let copied = copy_to_unique_destination(
                    workspace_root,
                    source_path,
                    media_format,
                    &content_hash,
                    cancellation,
                )?;
                (
                    copied.relative_path,
                    copied.absolute_path.clone(),
                    Some(copied.absolute_path),
                )
            }
        };
        if cancellation.is_cancelled() {
            if let Some(path) = copied_path.as_ref() {
                remove_copied_files(std::slice::from_ref(path))?;
            }
            return Err(ImportServiceError::Cancelled);
        }

        let file_size = match fs::metadata(&metadata_path) {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                if let Some(path) = copied_path.as_ref() {
                    remove_copied_files(std::slice::from_ref(path))?;
                }
                return Err(ImportServiceError::SourceUnavailable);
            }
        };
        let mut metadata = self
            .metadata_adapter
            .extract(&metadata_path, media_format.media_kind);
        if media_format.media_kind == MediaKind::Image {
            metadata.duration_ms = None;
        }
        let recognized_generation = self
            .metadata_adapter
            .recognize_generation(&metadata_path, media_format.media_kind);
        Ok(PreparedWorkerResult {
            index,
            item: PreparedMediaImport {
                media_kind: media_format.media_kind,
                import_mode,
                stored_path,
                file_name,
                mime_type: media_format.mime_type,
                file_size,
                content_hash,
                source_modified_at,
                metadata,
                recognized_generation,
            },
            copied_path,
        })
    }
}

fn ensure_writable(root: &Path) -> Result<(), ImportServiceError> {
    WriteAccessGuard::ensure_writable(root).map_err(|error| match error {
        AccessModeServiceError::ReadOnly => ImportServiceError::ReadOnly,
        _ => ImportServiceError::InvalidWorkspace,
    })
}

fn acquire_writable(root: &Path) -> Result<WriteAccessLease, ImportServiceError> {
    WriteAccessGuard::acquire_writable(root).map_err(|error| match error {
        AccessModeServiceError::ReadOnly => ImportServiceError::ReadOnly,
        _ => ImportServiceError::InvalidWorkspace,
    })
}

struct PreparedWorkerResult {
    index: usize,
    item: PreparedMediaImport,
    copied_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
struct MediaFormat {
    media_kind: MediaKind,
    extension: &'static str,
    mime_type: &'static str,
}

fn inspect_source(path: &Path) -> Result<MediaFormat, ImportServiceError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(ImportServiceError::InvalidSource);
    }
    let metadata = fs::metadata(path).map_err(|_| ImportServiceError::SourceUnavailable)?;
    if !metadata.is_file() {
        return Err(ImportServiceError::InvalidSource);
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(ImportServiceError::UnsupportedFormat)?;
    match extension.as_str() {
        "jpg" => Ok(MediaFormat {
            media_kind: MediaKind::Image,
            extension: "jpg",
            mime_type: "image/jpeg",
        }),
        "jpeg" => Ok(MediaFormat {
            media_kind: MediaKind::Image,
            extension: "jpeg",
            mime_type: "image/jpeg",
        }),
        "png" => Ok(MediaFormat {
            media_kind: MediaKind::Image,
            extension: "png",
            mime_type: "image/png",
        }),
        "webp" => Ok(MediaFormat {
            media_kind: MediaKind::Image,
            extension: "webp",
            mime_type: "image/webp",
        }),
        "gif" => Ok(MediaFormat {
            media_kind: MediaKind::Image,
            extension: "gif",
            mime_type: "image/gif",
        }),
        "mp4" => Ok(MediaFormat {
            media_kind: MediaKind::Video,
            extension: "mp4",
            mime_type: "video/mp4",
        }),
        _ => Err(ImportServiceError::UnsupportedFormat),
    }
}

struct CopiedDestination {
    absolute_path: PathBuf,
    relative_path: PathBuf,
}

fn copy_to_unique_destination(
    workspace_root: &Path,
    source_path: &Path,
    format: MediaFormat,
    expected_hash: &str,
    cancellation: &ImportCancellationToken,
) -> Result<CopiedDestination, ImportServiceError> {
    let folder = match format.media_kind {
        MediaKind::Image => "images",
        MediaKind::Video => "videos",
    };
    let destination_directory = workspace_root.join("media").join(folder);
    for suffix in 0..MAX_DESTINATION_ATTEMPTS {
        let file_name = if suffix == 0 {
            format!("{expected_hash}.{}", format.extension)
        } else {
            format!("{expected_hash}-{suffix}.{}", format.extension)
        };
        let absolute_path = destination_directory.join(&file_name);
        let destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&absolute_path);
        let mut destination = match destination {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ImportServiceError::CopyFailed),
        };
        let copy_result =
            copy_and_hash(source_path, &mut destination, cancellation).and_then(|actual_hash| {
                if actual_hash == expected_hash {
                    destination
                        .sync_all()
                        .map_err(|_| ImportServiceError::CopyFailed)
                } else {
                    Err(ImportServiceError::SourceChanged)
                }
            });
        drop(destination);
        if let Err(error) = copy_result {
            if fs::remove_file(&absolute_path).is_err() {
                return Err(ImportServiceError::CompensationFailed);
            }
            return Err(error);
        }
        return Ok(CopiedDestination {
            absolute_path,
            relative_path: PathBuf::from(format!("media/{folder}/{file_name}")),
        });
    }
    Err(ImportServiceError::CopyFailed)
}

fn copy_and_hash<W: Write>(
    source_path: &Path,
    destination: &mut W,
    cancellation: &ImportCancellationToken,
) -> Result<String, ImportServiceError> {
    let source = File::open(source_path).map_err(|_| ImportServiceError::SourceUnavailable)?;
    let mut reader = BufReader::with_capacity(STREAM_BUFFER_SIZE, source);
    let mut buffer = [0_u8; STREAM_BUFFER_SIZE];
    let mut hash = Sha256::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(ImportServiceError::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|_| ImportServiceError::ReadFailed)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        destination
            .write_all(&buffer[..read])
            .map_err(|_| ImportServiceError::CopyFailed)?;
    }
    Ok(hash.finalize_hex())
}

fn hash_file(
    path: &Path,
    cancellation: &ImportCancellationToken,
) -> Result<String, ImportServiceError> {
    let mut sink = std::io::sink();
    copy_and_hash(path, &mut sink, cancellation)
}

fn remove_copied_files(paths: &[PathBuf]) -> Result<(), ImportServiceError> {
    let mut failed = false;
    for path in paths.iter().rev() {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            failed = true;
        }
    }
    if failed {
        Err(ImportServiceError::CompensationFailed)
    } else {
        Ok(())
    }
}

// 小型标准库实现避免为单一流式哈希扩大发布依赖树。
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    total_bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            total_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(input.len() as u64);
        if self.buffered > 0 {
            let count = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + count].copy_from_slice(&input[..count]);
            self.buffered += count;
            input = &input[count..];
            if self.buffered < 64 {
                return;
            }
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while input.len() >= 64 {
            self.compress(&input[..64]);
            input = &input[64..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffer = [0; 64];
        } else {
            self.buffer[self.buffered..56].fill(0);
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);
        let mut output = String::with_capacity(64);
        for value in self.state {
            use std::fmt::Write as _;
            write!(&mut output, "{value:08x}").expect("写入 String 不会失败");
        }
        output
    }

    fn compress(&mut self, block: &[u8]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes(chunk.try_into().expect("块长度固定"));
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use crate::{
        adapters::media_metadata::MediaMetadataAdapter,
        domain::{ImportCancellationToken, ImportMode, MediaKind, MediaTechnicalMetadata},
    };

    use super::{
        ImportService, ImportServiceError, PrepareImportBatchRequest, Sha256, WorkspaceService,
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-import-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("应能创建导入测试目录");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn workspace_fixture(label: &str) -> (TestDirectory, PathBuf) {
        let parent = TestDirectory::new(label);
        let root = parent.path().join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建导入测试工作区");
        (parent, root)
    }

    fn write_png(path: &Path) {
        let mut bytes = vec![0_u8; 24];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[16..20].copy_from_slice(&320_u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&240_u32.to_be_bytes());
        fs::write(path, bytes).expect("应能写入 PNG 测试文件");
    }

    #[test]
    fn sha256_matches_known_vectors_across_chunks() {
        let mut hash = Sha256::new();
        hash.update(b"a");
        hash.update(b"bc");
        assert_eq!(
            hash.finalize_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reference_import_keeps_absolute_path_and_never_deletes_source() {
        let (parent, workspace_root) = workspace_fixture("reference");
        let source = parent.path().join("source.png");
        write_png(&source);
        let service = ImportService::new();
        let batch = service
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root,
                    import_mode: ImportMode::Reference,
                    source_paths: vec![source.clone()],
                },
                &ImportCancellationToken::default(),
            )
            .expect("引用导入应准备成功");

        assert_eq!(batch.items()[0].stored_path, source);
        assert_eq!(batch.items()[0].metadata.width, Some(320));
        assert!(
            batch.items()[0].source_modified_at.is_some(),
            "导入应读取原始文件的修改时间"
        );
        service
            .compensate_batch(&batch)
            .expect("引用补偿应为空操作");
        assert!(source.is_file(), "引用源文件绝不能被补偿删除");
    }

    #[test]
    fn copy_import_avoids_overwrite_and_compensates_only_new_files() {
        let (parent, workspace_root) = workspace_fixture("copy-conflict");
        let source = parent.path().join("same.png");
        write_png(&source);
        let service = ImportService::new();
        let batch = service
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: workspace_root.clone(),
                    import_mode: ImportMode::Copy,
                    source_paths: vec![source.clone(), source.clone()],
                },
                &ImportCancellationToken::default(),
            )
            .expect("相同文件也应以不覆盖路径完成准备");

        assert_ne!(batch.items()[0].stored_path, batch.items()[1].stored_path);
        assert!(
            batch
                .items()
                .iter()
                .all(|item| !item.stored_path.is_absolute())
        );
        assert_eq!(
            fs::read_dir(workspace_root.join("media/images"))
                .expect("应能读取受管图片目录")
                .count(),
            2
        );
        service.compensate_batch(&batch).expect("复制文件应可补偿");
        assert_eq!(
            fs::read_dir(workspace_root.join("media/images"))
                .expect("应能读取补偿后的图片目录")
                .count(),
            0
        );
        assert!(source.is_file(), "复制导入补偿不得删除原始源文件");
    }

    #[test]
    fn cancelled_or_unsupported_batches_leave_no_copied_files() {
        let (parent, workspace_root) = workspace_fixture("cancel");
        let source = parent.path().join("source.png");
        write_png(&source);
        let cancellation = ImportCancellationToken::default();
        cancellation.cancel();
        let error = ImportService::new()
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: workspace_root.clone(),
                    import_mode: ImportMode::Copy,
                    source_paths: vec![source],
                },
                &cancellation,
            )
            .expect_err("预先取消的批次必须停止");
        assert_eq!(error, ImportServiceError::Cancelled);

        let unsupported = parent.path().join("source.txt");
        fs::write(&unsupported, b"unsupported").expect("应能写入不支持格式");
        let error = ImportService::new()
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: workspace_root.clone(),
                    import_mode: ImportMode::Copy,
                    source_paths: vec![unsupported],
                },
                &ImportCancellationToken::default(),
            )
            .expect_err("不支持格式必须拒绝");
        assert_eq!(error, ImportServiceError::UnsupportedFormat);
        assert_eq!(
            fs::read_dir(workspace_root.join("media/images"))
                .expect("应能读取受管图片目录")
                .count(),
            0
        );
    }

    struct ConcurrencyProbe {
        active: AtomicUsize,
        maximum: AtomicUsize,
    }

    struct DurationProbe;

    impl MediaMetadataAdapter for DurationProbe {
        fn extract(&self, _path: &Path, _media_kind: MediaKind) -> MediaTechnicalMetadata {
            MediaTechnicalMetadata {
                duration_ms: Some(12_345),
                ..MediaTechnicalMetadata::default()
            }
        }
    }

    #[test]
    fn import_keeps_duration_only_for_videos_after_format_detection() {
        let (parent, workspace_root) = workspace_fixture("duration-by-format");
        let image = parent.path().join("duration.png");
        let video = parent.path().join("duration.mp4");
        write_png(&image);
        fs::write(&video, b"video fixture").expect("应能写入视频测试文件");

        let batch = ImportService::with_metadata_adapter(Arc::new(DurationProbe))
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root,
                    import_mode: ImportMode::Reference,
                    source_paths: vec![image, video],
                },
                &ImportCancellationToken::default(),
            )
            .expect("图片和视频应能完成格式识别与导入准备");

        assert_eq!(batch.items()[0].media_kind, MediaKind::Image);
        assert_eq!(batch.items()[0].metadata.duration_ms, None);
        assert_eq!(batch.items()[1].media_kind, MediaKind::Video);
        assert_eq!(batch.items()[1].metadata.duration_ms, Some(12_345));
    }

    impl MediaMetadataAdapter for ConcurrencyProbe {
        fn extract(&self, _path: &Path, _media_kind: MediaKind) -> MediaTechnicalMetadata {
            let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
            self.maximum.fetch_max(active, Ordering::AcqRel);
            thread::sleep(Duration::from_millis(20));
            self.active.fetch_sub(1, Ordering::AcqRel);
            MediaTechnicalMetadata::default()
        }
    }

    #[test]
    fn batch_worker_concurrency_never_exceeds_two() {
        let (parent, workspace_root) = workspace_fixture("concurrency");
        let mut sources = Vec::new();
        for index in 0..4 {
            let source = parent.path().join(format!("source-{index}.png"));
            write_png(&source);
            sources.push(source);
        }
        let probe = Arc::new(ConcurrencyProbe {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        let service = ImportService::with_metadata_adapter(probe.clone());
        let batch = service
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root,
                    import_mode: ImportMode::Reference,
                    source_paths: sources,
                },
                &ImportCancellationToken::default(),
            )
            .expect("并发探针批次应成功");
        assert_eq!(batch.items().len(), 4);
        assert!(probe.maximum.load(Ordering::Acquire) <= 2);
    }

    #[test]
    fn progress_callback_reports_each_completed_file_without_full_path() {
        let (parent, workspace_root) = workspace_fixture("progress");
        let sources = (0..3)
            .map(|index| {
                let source = parent.path().join(format!("progress-{index}.png"));
                write_png(&source);
                source
            })
            .collect();
        let updates = Mutex::new(Vec::new());

        let batch = ImportService::new()
            .prepare_batch_with_progress(
                PrepareImportBatchRequest {
                    workspace_root,
                    import_mode: ImportMode::Reference,
                    source_paths: sources,
                },
                &ImportCancellationToken::default(),
                &|completed, file_name| {
                    updates
                        .lock()
                        .expect("进度记录锁不应中毒")
                        .push((completed, file_name.to_owned()));
                },
            )
            .expect("进度回调批次应成功");

        let mut updates = updates.into_inner().expect("应能读取进度记录");
        updates.sort_unstable_by_key(|(completed, _)| *completed);
        assert_eq!(batch.items().len(), 3);
        assert_eq!(
            updates.iter().map(|item| item.0).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(updates.iter().all(|(_, name)| !name.contains(['/', '\\'])));
    }
}
