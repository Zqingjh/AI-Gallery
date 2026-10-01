use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::domain::{MediaKind, MediaTechnicalMetadata, RecognizedGenerationMetadata};

const MAX_EMBEDDED_METADATA_BYTES: u32 = 1024 * 1024;

pub(crate) trait MediaMetadataAdapter: Send + Sync {
    fn extract(&self, path: &Path, media_kind: MediaKind) -> MediaTechnicalMetadata;

    fn recognize_generation(
        &self,
        _path: &Path,
        _media_kind: MediaKind,
    ) -> RecognizedGenerationMetadata {
        RecognizedGenerationMetadata::default()
    }
}

#[derive(Debug, Default)]
pub(crate) struct BasicMediaMetadataAdapter;

impl MediaMetadataAdapter for BasicMediaMetadataAdapter {
    fn extract(&self, path: &Path, media_kind: MediaKind) -> MediaTechnicalMetadata {
        match media_kind {
            MediaKind::Image => image_dimensions(path)
                .map(|(width, height)| MediaTechnicalMetadata {
                    width: Some(width),
                    height: Some(height),
                    ..MediaTechnicalMetadata::default()
                })
                .unwrap_or_default(),
            MediaKind::Video => mp4_metadata(path).unwrap_or_default(),
        }
    }

    fn recognize_generation(
        &self,
        path: &Path,
        media_kind: MediaKind,
    ) -> RecognizedGenerationMetadata {
        if media_kind == MediaKind::Image {
            png_generation_metadata(path).unwrap_or_default()
        } else {
            RecognizedGenerationMetadata::default()
        }
    }
}

fn png_generation_metadata(path: &Path) -> Option<RecognizedGenerationMetadata> {
    let mut file = File::open(path).ok()?;
    let mut signature = [0_u8; 8];
    file.read_exact(&mut signature).ok()?;
    if signature != *b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    loop {
        let mut header = [0_u8; 8];
        file.read_exact(&mut header).ok()?;
        let length = u32::from_be_bytes(header[..4].try_into().ok()?);
        let kind = &header[4..8];
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        if kind == b"tEXt" && length <= MAX_EMBEDDED_METADATA_BYTES {
            let mut payload = vec![0_u8; length as usize];
            file.read_exact(&mut payload).ok()?;
            file.seek(SeekFrom::Current(4)).ok()?;
            let separator = payload.iter().position(|byte| *byte == 0)?;
            let keyword = std::str::from_utf8(&payload[..separator]).ok()?;
            if matches!(keyword, "parameters" | "prompt") {
                let value = String::from_utf8(payload[separator + 1..].to_vec()).ok()?;
                return Some(parse_generation_parameters(&value));
            }
        } else {
            file.seek(SeekFrom::Current(i64::from(length) + 4)).ok()?;
        }
    }
}

fn parse_generation_parameters(value: &str) -> RecognizedGenerationMetadata {
    let (positive, remainder) = value
        .split_once("\nNegative prompt: ")
        .map_or((value, ""), |(positive, remainder)| (positive, remainder));
    let (negative, settings) = remainder
        .rsplit_once("\nSteps: ")
        .map_or((remainder, ""), |(negative, settings)| (negative, settings));
    let mut generation_params = serde_json::Map::new();
    if !settings.is_empty() {
        generation_params.insert(
            "Steps".to_owned(),
            json_scalar(settings.split(',').next().unwrap_or("")),
        );
        for pair in settings.split(',').skip(1) {
            if let Some((key, value)) = pair.trim().split_once(": ") {
                generation_params.insert(key.to_owned(), json_scalar(value));
            }
        }
    }
    let positive = positive.trim().to_owned();
    let contains_cjk = positive
        .chars()
        .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character));
    RecognizedGenerationMetadata {
        prompt_zh: if contains_cjk {
            positive.clone()
        } else {
            String::new()
        },
        prompt_en: if contains_cjk {
            String::new()
        } else {
            positive
        },
        negative_prompt: negative.trim().to_owned(),
        generation_params: serde_json::Value::Object(generation_params),
    }
}

fn json_scalar(value: &str) -> serde_json::Value {
    let value = value.trim();
    value
        .parse::<i64>()
        .map(serde_json::Value::from)
        .or_else(|_| value.parse::<f64>().map(serde_json::Value::from))
        .unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

fn image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut file = File::open(path).ok()?;
    let mut header = [0_u8; 30];
    let read = file.read(&mut header).ok()?;
    if read >= 24 && header[..8] == *b"\x89PNG\r\n\x1a\n" {
        return positive_dimensions(
            u32::from_be_bytes(header[16..20].try_into().ok()?),
            u32::from_be_bytes(header[20..24].try_into().ok()?),
        );
    }
    if read >= 10 && matches!(&header[..6], b"GIF87a" | b"GIF89a") {
        return positive_dimensions(
            u16::from_le_bytes(header[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(header[8..10].try_into().ok()?) as u32,
        );
    }
    if read >= 30 && &header[..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return webp_dimensions(&header[..read]);
    }
    if read >= 2 && header[..2] == [0xff, 0xd8] {
        file.seek(SeekFrom::Start(2)).ok()?;
        return jpeg_dimensions(&mut file);
    }
    None
}

fn positive_dimensions(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

fn webp_dimensions(header: &[u8]) -> Option<(u32, u32)> {
    match header.get(12..16)? {
        b"VP8X" if header.len() >= 30 => positive_dimensions(
            1 + u32::from_le_bytes([header[24], header[25], header[26], 0]),
            1 + u32::from_le_bytes([header[27], header[28], header[29], 0]),
        ),
        b"VP8L" if header.len() >= 25 && header[20] == 0x2f => {
            let width = 1 + u32::from(header[21]) + (u32::from(header[22] & 0x3f) << 8);
            let height = 1
                + u32::from(header[22] >> 6)
                + (u32::from(header[23]) << 2)
                + (u32::from(header[24] & 0x0f) << 10);
            positive_dimensions(width, height)
        }
        b"VP8 " if header.len() >= 30 && header[23..26] == [0x9d, 0x01, 0x2a] => {
            positive_dimensions(
                u16::from_le_bytes([header[26], header[27]]) as u32 & 0x3fff,
                u16::from_le_bytes([header[28], header[29]]) as u32 & 0x3fff,
            )
        }
        _ => None,
    }
}

fn jpeg_dimensions(file: &mut File) -> Option<(u32, u32)> {
    loop {
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte).ok()?;
        if byte[0] != 0xff {
            continue;
        }
        while byte[0] == 0xff {
            file.read_exact(&mut byte).ok()?;
        }
        let marker = byte[0];
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let mut length = [0_u8; 2];
        file.read_exact(&mut length).ok()?;
        let segment_length = u16::from_be_bytes(length);
        if segment_length < 2 {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if segment_length < 7 {
                return None;
            }
            let mut dimensions = [0_u8; 5];
            file.read_exact(&mut dimensions).ok()?;
            return positive_dimensions(
                u16::from_be_bytes([dimensions[3], dimensions[4]]) as u32,
                u16::from_be_bytes([dimensions[1], dimensions[2]]) as u32,
            );
        }
        file.seek(SeekFrom::Current(i64::from(segment_length) - 2))
            .ok()?;
    }
}

fn mp4_metadata(path: &Path) -> Option<MediaTechnicalMetadata> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut cursor = 0_u64;
    let mut metadata = MediaTechnicalMetadata::default();
    while cursor + 8 <= file_len {
        let header = read_box_header(&mut file, cursor, file_len)?;
        if &header.kind == b"moov" {
            parse_mp4_children(
                &mut file,
                header.content_start,
                header.end,
                &mut metadata,
                0,
            )?;
            metadata.has_audio.get_or_insert(false);
            return Some(metadata);
        }
        cursor = header.end;
    }
    None
}

struct BoxHeader {
    kind: [u8; 4],
    content_start: u64,
    end: u64,
}

#[derive(Default)]
struct Mp4TrackMetadata {
    handler: Option<[u8; 4]>,
    timescale: Option<u32>,
    sample_count: Option<u64>,
    sample_duration: Option<u64>,
    dimensions: Option<(u32, u32)>,
}

fn read_box_header(file: &mut File, start: u64, parent_end: u64) -> Option<BoxHeader> {
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes).ok()?;
    let size32 = u32::from_be_bytes(bytes[..4].try_into().ok()?) as u64;
    let kind = bytes[4..8].try_into().ok()?;
    let (size, header_len) = if size32 == 1 {
        let mut extended = [0_u8; 8];
        file.read_exact(&mut extended).ok()?;
        (u64::from_be_bytes(extended), 16_u64)
    } else if size32 == 0 {
        (parent_end.checked_sub(start)?, 8_u64)
    } else {
        (size32, 8_u64)
    };
    if size < header_len {
        return None;
    }
    let end = start.checked_add(size)?;
    if end > parent_end {
        return None;
    }
    Some(BoxHeader {
        kind,
        content_start: start + header_len,
        end,
    })
}

fn parse_mp4_children(
    file: &mut File,
    start: u64,
    end: u64,
    metadata: &mut MediaTechnicalMetadata,
    depth: u8,
) -> Option<()> {
    if depth > 5 {
        return Some(());
    }
    let mut cursor = start;
    while cursor + 8 <= end {
        let header = read_box_header(file, cursor, end)?;
        match &header.kind {
            b"mvhd" => metadata.duration_ms = read_mvhd(file, &header),
            b"trak" => {
                let mut track = Mp4TrackMetadata::default();
                parse_mp4_track_children(
                    file,
                    header.content_start,
                    header.end,
                    &mut track,
                    depth + 1,
                )?;
                match track.handler.as_ref() {
                    Some(b"soun") => metadata.has_audio = Some(true),
                    Some(b"vide") => {
                        if let Some((width, height)) = track.dimensions {
                            metadata.width.get_or_insert(width);
                            metadata.height.get_or_insert(height);
                        }
                        if let (Some(timescale), Some(samples), Some(duration)) =
                            (track.timescale, track.sample_count, track.sample_duration)
                        {
                            if duration > 0 {
                                let frame_rate =
                                    samples as f64 * f64::from(timescale) / duration as f64;
                                if frame_rate.is_finite() && frame_rate > 0.0 {
                                    metadata.frame_rate.get_or_insert(frame_rate);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        cursor = header.end;
    }
    Some(())
}

fn parse_mp4_track_children(
    file: &mut File,
    start: u64,
    end: u64,
    track: &mut Mp4TrackMetadata,
    depth: u8,
) -> Option<()> {
    if depth > 6 {
        return Some(());
    }
    let mut cursor = start;
    while cursor + 8 <= end {
        let header = read_box_header(file, cursor, end)?;
        match &header.kind {
            b"tkhd" => track.dimensions = read_tkhd_dimensions(file, &header),
            b"mdhd" => track.timescale = read_mdhd_timescale(file, &header),
            b"hdlr" => track.handler = read_handler(file, &header),
            b"stts" => {
                if let Some((samples, duration)) = read_stts(file, &header) {
                    track.sample_count = Some(samples);
                    track.sample_duration = Some(duration);
                }
            }
            b"mdia" | b"minf" | b"stbl" => {
                parse_mp4_track_children(file, header.content_start, header.end, track, depth + 1)?
            }
            _ => {}
        }
        cursor = header.end;
    }
    Some(())
}

fn read_mdhd_timescale(file: &mut File, header: &BoxHeader) -> Option<u32> {
    file.seek(SeekFrom::Start(header.content_start)).ok()?;
    let mut version = [0_u8; 1];
    file.read_exact(&mut version).ok()?;
    let offset = if version[0] == 1 { 20_u64 } else { 12_u64 };
    if header.content_start + offset + 4 > header.end {
        return None;
    }
    file.seek(SeekFrom::Start(header.content_start + offset))
        .ok()?;
    let mut value = [0_u8; 4];
    file.read_exact(&mut value).ok()?;
    let timescale = u32::from_be_bytes(value);
    (timescale > 0).then_some(timescale)
}

fn read_handler(file: &mut File, header: &BoxHeader) -> Option<[u8; 4]> {
    if header.content_start + 12 > header.end {
        return None;
    }
    file.seek(SeekFrom::Start(header.content_start + 8)).ok()?;
    let mut handler = [0_u8; 4];
    file.read_exact(&mut handler).ok()?;
    Some(handler)
}

fn read_stts(file: &mut File, header: &BoxHeader) -> Option<(u64, u64)> {
    if header.content_start + 8 > header.end {
        return None;
    }
    file.seek(SeekFrom::Start(header.content_start + 4)).ok()?;
    let mut count = [0_u8; 4];
    file.read_exact(&mut count).ok()?;
    let entry_count = u32::from_be_bytes(count);
    if entry_count > 100_000 || header.content_start + 8 + u64::from(entry_count) * 8 > header.end {
        return None;
    }
    let mut total_samples = 0_u64;
    let mut total_duration = 0_u64;
    for _ in 0..entry_count {
        let mut entry = [0_u8; 8];
        file.read_exact(&mut entry).ok()?;
        let samples = u64::from(u32::from_be_bytes(entry[..4].try_into().ok()?));
        let delta = u64::from(u32::from_be_bytes(entry[4..].try_into().ok()?));
        total_samples = total_samples.checked_add(samples)?;
        total_duration = total_duration.checked_add(samples.checked_mul(delta)?)?;
    }
    (total_samples > 0 && total_duration > 0).then_some((total_samples, total_duration))
}

fn read_mvhd(file: &mut File, header: &BoxHeader) -> Option<u64> {
    file.seek(SeekFrom::Start(header.content_start)).ok()?;
    let mut version = [0_u8; 1];
    file.read_exact(&mut version).ok()?;
    let (timescale_offset, duration_offset, duration_len) = if version[0] == 1 {
        (20_u64, 24_u64, 8_usize)
    } else {
        (12_u64, 16_u64, 4_usize)
    };
    if header.content_start + duration_offset + duration_len as u64 > header.end {
        return None;
    }
    file.seek(SeekFrom::Start(header.content_start + timescale_offset))
        .ok()?;
    let mut timescale = [0_u8; 4];
    file.read_exact(&mut timescale).ok()?;
    let timescale = u32::from_be_bytes(timescale) as u64;
    let mut duration = [0_u8; 8];
    file.seek(SeekFrom::Start(header.content_start + duration_offset))
        .ok()?;
    file.read_exact(&mut duration[..duration_len]).ok()?;
    let duration = if duration_len == 8 {
        u64::from_be_bytes(duration)
    } else {
        u32::from_be_bytes(duration[..4].try_into().ok()?) as u64
    };
    (timescale > 0).then(|| duration.saturating_mul(1_000) / timescale)
}

fn read_tkhd_dimensions(file: &mut File, header: &BoxHeader) -> Option<(u32, u32)> {
    if header.end < header.content_start + 8 {
        return None;
    }
    file.seek(SeekFrom::Start(header.end - 8)).ok()?;
    let mut dimensions = [0_u8; 8];
    file.read_exact(&mut dimensions).ok()?;
    positive_dimensions(
        u32::from_be_bytes(dimensions[..4].try_into().ok()?) >> 16,
        u32::from_be_bytes(dimensions[4..].try_into().ok()?) >> 16,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{BasicMediaMetadataAdapter, MediaMetadataAdapter};
    use crate::domain::MediaKind;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fixture(extension: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ai-gallery-metadata-{}-{}.{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            extension
        ));
        fs::write(&path, bytes).expect("应能写入元信息测试文件");
        path
    }

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(payload.len() + 8);
        bytes.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn png_text(keyword: &str, value: &str) -> Vec<u8> {
        let mut payload = keyword.as_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(value.as_bytes());
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(b"tEXt");
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&[0_u8; 4]);
        bytes
    }

    #[test]
    fn reads_png_and_gif_dimensions_without_decoding_pixels() {
        let mut png = vec![0_u8; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[16..20].copy_from_slice(&640_u32.to_be_bytes());
        png[20..24].copy_from_slice(&480_u32.to_be_bytes());
        let png_path = fixture("png", &png);
        let gif_path = fixture("gif", b"GIF89a\x20\x03\x58\x02");
        let adapter = BasicMediaMetadataAdapter;

        let png_metadata = adapter.extract(&png_path, MediaKind::Image);
        let gif_metadata = adapter.extract(&gif_path, MediaKind::Image);

        assert_eq!(
            (png_metadata.width, png_metadata.height),
            (Some(640), Some(480))
        );
        assert_eq!(
            (gif_metadata.width, gif_metadata.height),
            (Some(800), Some(600))
        );
        let _ = fs::remove_file(png_path);
        let _ = fs::remove_file(gif_path);
    }

    #[test]
    fn malformed_media_degrades_to_empty_metadata() {
        let path = fixture("mp4", b"not-an-mp4");
        let metadata = BasicMediaMetadataAdapter.extract(&path, MediaKind::Video);
        assert_eq!(metadata.width, None);
        assert_eq!(metadata.duration_ms, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recognizes_bounded_png_generation_parameters_without_decoding_image() {
        let path = fixture(
            "png",
            &png_text(
                "parameters",
                "future city at dawn\nNegative prompt: blur\nSteps: 24, Sampler: Euler, CFG scale: 7.5, Seed: 42",
            ),
        );

        let recognized = BasicMediaMetadataAdapter.recognize_generation(&path, MediaKind::Image);

        assert_eq!(recognized.prompt_en, "future city at dawn");
        assert_eq!(recognized.negative_prompt, "blur");
        assert_eq!(recognized.generation_params["Steps"], 24);
        assert_eq!(recognized.generation_params["Seed"], 42);
        assert_eq!(recognized.generation_params["CFG scale"], 7.5);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reads_basic_mp4_movie_duration_and_track_dimensions() {
        let mut mvhd_payload = vec![0_u8; 20];
        mvhd_payload[12..16].copy_from_slice(&30_u32.to_be_bytes());
        mvhd_payload[16..20].copy_from_slice(&90_u32.to_be_bytes());
        let mvhd = mp4_box(b"mvhd", &mvhd_payload);

        let mut tkhd_payload = vec![0_u8; 84];
        tkhd_payload[76..80].copy_from_slice(&(1920_u32 << 16).to_be_bytes());
        tkhd_payload[80..84].copy_from_slice(&(1080_u32 << 16).to_be_bytes());
        let mut mdhd_payload = vec![0_u8; 16];
        mdhd_payload[12..16].copy_from_slice(&30_u32.to_be_bytes());
        let mut video_handler = vec![0_u8; 12];
        video_handler[8..12].copy_from_slice(b"vide");
        let mut stts_payload = vec![0_u8; 16];
        stts_payload[4..8].copy_from_slice(&1_u32.to_be_bytes());
        stts_payload[8..12].copy_from_slice(&90_u32.to_be_bytes());
        stts_payload[12..16].copy_from_slice(&1_u32.to_be_bytes());
        let mut video_mdia = mp4_box(b"mdhd", &mdhd_payload);
        video_mdia.extend_from_slice(&mp4_box(b"hdlr", &video_handler));
        video_mdia.extend_from_slice(&mp4_box(
            b"minf",
            &mp4_box(b"stbl", &mp4_box(b"stts", &stts_payload)),
        ));
        let mut video_track = mp4_box(b"tkhd", &tkhd_payload);
        video_track.extend_from_slice(&mp4_box(b"mdia", &video_mdia));
        let trak = mp4_box(b"trak", &video_track);

        let mut audio_handler = vec![0_u8; 12];
        audio_handler[8..12].copy_from_slice(b"soun");
        let audio_track = mp4_box(
            b"trak",
            &mp4_box(b"mdia", &mp4_box(b"hdlr", &audio_handler)),
        );
        let mut moov_payload = mvhd;
        moov_payload.extend_from_slice(&trak);
        moov_payload.extend_from_slice(&audio_track);
        let path = fixture("mp4", &mp4_box(b"moov", &moov_payload));

        let metadata = BasicMediaMetadataAdapter.extract(&path, MediaKind::Video);

        assert_eq!(metadata.duration_ms, Some(3_000));
        assert_eq!((metadata.width, metadata.height), (Some(1920), Some(1080)));
        assert_eq!(metadata.frame_rate, Some(30.0));
        assert_eq!(metadata.has_audio, Some(true));
        let _ = fs::remove_file(path);
    }
}
