use std::{
    fmt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportMode {
    Copy,
    Reference,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MediaTechnicalMetadata {
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) frame_rate: Option<f64>,
    pub(crate) has_audio: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecognizedGenerationMetadata {
    pub(crate) prompt_zh: String,
    pub(crate) prompt_en: String,
    pub(crate) negative_prompt: String,
    pub(crate) generation_params: serde_json::Value,
}

impl Default for RecognizedGenerationMetadata {
    fn default() -> Self {
        Self {
            prompt_zh: String::new(),
            prompt_en: String::new(),
            negative_prompt: String::new(),
            generation_params: serde_json::json!({}),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PreparedMediaImport {
    pub(crate) media_kind: MediaKind,
    pub(crate) import_mode: ImportMode,
    pub(crate) stored_path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) mime_type: &'static str,
    pub(crate) file_size: u64,
    pub(crate) content_hash: String,
    /// 导入源文件的修改时间（毫秒）；读取失败时由写入层使用导入时间。
    pub(crate) source_modified_at: Option<i64>,
    pub(crate) metadata: MediaTechnicalMetadata,
    pub(crate) recognized_generation: RecognizedGenerationMetadata,
}

// 路径可能是外部绝对路径，因此调试输出必须始终脱敏。
impl fmt::Debug for PreparedMediaImport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedMediaImport")
            .field("media_kind", &self.media_kind)
            .field("import_mode", &self.import_mode)
            .field("stored_path", &"<redacted>")
            .field("file_name", &self.file_name)
            .field("mime_type", &self.mime_type)
            .field("file_size", &self.file_size)
            .field("content_hash", &self.content_hash)
            .field("metadata", &self.metadata)
            .field(
                "recognized_generation",
                &(!self.recognized_generation.prompt_zh.is_empty()
                    || !self.recognized_generation.prompt_en.is_empty()
                    || !self.recognized_generation.negative_prompt.is_empty()
                    || self.recognized_generation.generation_params != serde_json::json!({})),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ImportCancellationToken(Arc<AtomicBool>);

impl ImportCancellationToken {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
