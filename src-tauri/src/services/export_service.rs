use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        AssetCover, AssetDetail, AssetListQuery, CustomField, CustomFieldTargetType,
        CustomFieldValue, MediaType, PageCursor, PathAvailability, PathKind, ProjectDetail,
        ProjectKind, StoredPathKind,
    },
    services::{
        LibraryService, LibraryServiceError, MediaIntegrityService, P1LibraryService,
        P1LibraryServiceError, WorkspaceService, WorkspaceServiceError,
    },
};

const MAX_SELECTED_ENTITIES: usize = 100;
const MAX_EXPORTED_ASSETS: usize = 10_000;
const MAX_EXPORTED_CANVAS_MEMBERS: usize = 10_000;
const CUSTOM_FIELD_TARGET_BATCH_SIZE: usize = 500;
const PAGE_LIMIT: u32 = 100;
static EXPORT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ExportMode {
    Complete,
    Prompts,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExportSelectionRequest {
    pub(crate) root_path: String,
    pub(crate) target_directory: String,
    pub(crate) mode: ExportMode,
    pub(crate) asset_ids: Option<Vec<i64>>,
    pub(crate) project_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportServiceError {
    InvalidInput,
    SourceUnavailable,
    DestinationUnavailable,
    Library(LibraryServiceError),
    CustomFields(P1LibraryServiceError),
    Workspace(WorkspaceServiceError),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptProject {
    id: i64,
    title: String,
    prompt: crate::domain::PromptText,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptAsset {
    id: i64,
    file_name: String,
    project_id: Option<i64>,
    prompt: crate::domain::PromptText,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptCanvasMember {
    project_id: i64,
    asset_id: i64,
    role: crate::domain::CanvasMemberRole,
    reference_name: Option<String>,
    prompt_zh: String,
    prompt_en: String,
    negative_prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportCanvasMember {
    project_id: i64,
    asset_id: i64,
    display_order: i64,
    file_name: String,
    media_type: crate::domain::MediaType,
    model_name: Option<String>,
    platform_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration_ms: Option<i64>,
    updated_at: i64,
    role: crate::domain::CanvasMemberRole,
    reference_name: Option<String>,
    prompt_zh: String,
    prompt_en: String,
    negative_prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportedCover {
    cover: AssetCover,
    path: String,
}

#[derive(Default)]
struct ExportContents {
    projects: Vec<ProjectDetail>,
    asset_ids: Vec<i64>,
    selected_assets: Vec<AssetDetail>,
    canvas_project_ids: Vec<i64>,
    canvas_member_asset_ids: Option<HashSet<i64>>,
    custom_fields: Vec<CustomField>,
    custom_field_values: Vec<CustomFieldValue>,
}

pub(crate) struct ExportService;

impl ExportService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn export_selection(
        &self,
        request: ExportSelectionRequest,
    ) -> Result<String, ExportServiceError> {
        let selected_assets = request.asset_ids.is_some();
        let selected_projects = request.project_ids.is_some();
        if selected_assets == selected_projects {
            return Err(ExportServiceError::InvalidInput);
        }
        if let Some(ids) = request.asset_ids.as_deref() {
            validate_ids(ids)?;
        }
        if let Some(ids) = request.project_ids.as_deref() {
            validate_ids(ids)?;
        }

        let root = Path::new(&request.root_path);
        let workspace = WorkspaceService::new();
        workspace
            .open_workspace(root)
            .map_err(ExportServiceError::Workspace)?;
        let root = fs::canonicalize(root).map_err(|_| ExportServiceError::SourceUnavailable)?;
        let target = validate_target_directory(&root, Path::new(&request.target_directory))?;
        let library = LibraryService::new();
        let mut contents = collect_contents(&library, &root, &request)?;
        if request.mode == ExportMode::Complete {
            collect_custom_field_data(&root, &mut contents)?;
        }

        match request.mode {
            ExportMode::Complete => export_complete(&library, &root, &target, contents),
            ExportMode::Prompts => export_prompts(&library, &root, &target, contents),
        }
    }
}

fn collect_contents(
    library: &LibraryService,
    root: &Path,
    request: &ExportSelectionRequest,
) -> Result<ExportContents, ExportServiceError> {
    let mut project_ids = BTreeSet::new();
    let mut asset_ids = BTreeSet::new();
    let mut projects = Vec::new();
    let mut canvas_project_ids = Vec::new();
    let mut canvas_member_count = 0;

    if let Some(ids) = request.project_ids.as_deref() {
        for id in ids {
            let project = library
                .get_project(root, *id)
                .map_err(ExportServiceError::Library)?;
            project_ids.insert(*id);
            if project.summary.kind == ProjectKind::Canvas {
                canvas_project_ids.push(*id);
            }
            projects.push(project);
        }

        for project_id in ids {
            let mut cursor = None;
            loop {
                let page = library
                    .list_assets(
                        root,
                        AssetListQuery {
                            project_id: Some(*project_id),
                            cursor,
                            limit: PAGE_LIMIT,
                            ..AssetListQuery::default()
                        },
                    )
                    .map_err(ExportServiceError::Library)?;
                for asset in page.items {
                    asset_ids.insert(asset.id);
                    if asset_ids.len() > MAX_EXPORTED_ASSETS {
                        return Err(ExportServiceError::InvalidInput);
                    }
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }

            if !canvas_project_ids.contains(project_id) {
                continue;
            }
            let mut cursor = None;
            loop {
                let page = library
                    .list_canvas_members(root, *project_id, cursor, PAGE_LIMIT, None)
                    .map_err(ExportServiceError::Library)?;
                for member in page.items {
                    canvas_member_count += 1;
                    if canvas_member_count > MAX_EXPORTED_CANVAS_MEMBERS {
                        return Err(ExportServiceError::InvalidInput);
                    }
                    asset_ids.insert(member.asset_id);
                    if asset_ids.len() > MAX_EXPORTED_ASSETS {
                        return Err(ExportServiceError::InvalidInput);
                    }
                }
                cursor = page.next_cursor.map(|value| PageCursor {
                    updated_at: value.updated_at,
                    id: value.id,
                });
                if cursor.is_none() {
                    break;
                }
            }
        }
    } else if let Some(ids) = request.asset_ids.as_deref() {
        asset_ids.extend(ids.iter().copied());
    }

    let asset_ids = asset_ids.into_iter().collect::<Vec<_>>();
    let selected_assets = if request.asset_ids.is_some() {
        library
            .get_asset_details_for_export(root, &asset_ids)
            .map_err(ExportServiceError::Library)?
    } else {
        Vec::new()
    };

    if request.project_ids.is_none() {
        for project_id in selected_assets
            .iter()
            .filter_map(|asset| asset.summary.media.project_id)
        {
            if project_ids.insert(project_id) {
                let project = library
                    .get_project(root, project_id)
                    .map_err(ExportServiceError::Library)?;
                if project.summary.kind == ProjectKind::Canvas {
                    canvas_project_ids.push(project_id);
                }
                projects.push(project);
            }
        }
    }

    canvas_project_ids.sort_unstable();
    canvas_project_ids.dedup();
    let canvas_member_asset_ids = request
        .asset_ids
        .as_ref()
        .map(|_| asset_ids.iter().copied().collect::<HashSet<_>>());

    Ok(ExportContents {
        projects,
        asset_ids,
        selected_assets,
        canvas_project_ids,
        canvas_member_asset_ids,
        custom_fields: Vec::new(),
        custom_field_values: Vec::new(),
    })
}

fn collect_custom_field_data(
    root: &Path,
    contents: &mut ExportContents,
) -> Result<(), ExportServiceError> {
    let service = P1LibraryService::new();
    let mut used_field_ids = HashSet::new();
    let mut values = Vec::new();

    for (target, target_ids) in [
        (
            CustomFieldTargetType::Project,
            contents
                .projects
                .iter()
                .map(|project| project.summary.id)
                .collect::<Vec<_>>(),
        ),
        (CustomFieldTargetType::Asset, contents.asset_ids.clone()),
    ] {
        for target_batch in target_ids.chunks(CUSTOM_FIELD_TARGET_BATCH_SIZE) {
            let mut cursor = None;
            loop {
                let page = service
                    .list_custom_field_values_for_targets(
                        root,
                        target,
                        target_batch,
                        cursor,
                        PAGE_LIMIT,
                    )
                    .map_err(ExportServiceError::CustomFields)?;
                used_field_ids.extend(page.items.iter().map(|value| value.field_id));
                values.extend(page.items);
                cursor = page.next_cursor.map(|value| PageCursor {
                    updated_at: value.updated_at,
                    id: value.id,
                });
                if cursor.is_none() {
                    break;
                }
            }
        }
    }

    let mut fields = Vec::new();
    for target in [CustomFieldTargetType::Project, CustomFieldTargetType::Asset] {
        let mut cursor = None;
        loop {
            let page = service
                .list_custom_fields(root, target, cursor, PAGE_LIMIT)
                .map_err(ExportServiceError::CustomFields)?;
            fields.extend(
                page.items
                    .into_iter()
                    .filter(|field| used_field_ids.contains(&field.id)),
            );
            cursor = page.next_cursor.map(|value| PageCursor {
                updated_at: value.updated_at,
                id: value.id,
            });
            if cursor.is_none() {
                break;
            }
        }
    }

    contents.custom_fields = fields;
    contents.custom_field_values = values;
    Ok(())
}

fn export_complete(
    library: &LibraryService,
    root: &Path,
    target: &Path,
    contents: ExportContents,
) -> Result<String, ExportServiceError> {
    let mut category_ids = BTreeSet::new();
    let mut tag_ids = BTreeSet::new();
    for project in &contents.projects {
        category_ids.extend(project.category_ids.iter().copied());
        tag_ids.extend(project.tag_ids.iter().copied());
    }

    let (staging, final_directory) = create_export_directory(target)?;
    let export_result = (|| {
        let media_directory = staging.join("media");
        fs::create_dir(&media_directory).map_err(|_| ExportServiceError::DestinationUnavailable)?;
        let manifest_path = staging.join("manifest.json");
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&manifest_path)
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write!(
            manifest,
            "{{\n  \"formatVersion\": 1,\n  \"exportedAt\": {},\n  \"projects\": ",
            current_time_millis()?
        )
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &contents.projects)?;
        manifest
            .write_all(b",\n  \"assets\": [")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;

        let media_integrity = MediaIntegrityService::new();
        let mut exported_covers = Vec::new();
        let mut first_asset = true;
        for_each_asset(
            library,
            root,
            &contents.asset_ids,
            &contents.selected_assets,
            |mut asset| {
                category_ids.extend(asset.category_ids.iter().copied());
                tag_ids.extend(asset.tag_ids.iter().copied());
                let source = resolve_source_path(&WorkspaceService::new(), root, &asset)?;
                let output_name = format!(
                    "asset_{}_{}",
                    asset.summary.id,
                    safe_file_name(&asset.summary.media.file_name)
                );
                let relative_path = format!("media/{output_name}");
                let target_path = media_directory.join(&output_name);
                let copied_size = copy_media_file(root, &source, &target_path, &asset)?;
                if asset.summary.media.media_type == MediaType::Video {
                    let cover = media_integrity
                        .get_asset_cover(root, asset.summary.id)
                        .map_err(|_| ExportServiceError::SourceUnavailable)?;
                    if let Some(cover) = cover {
                        let cover_directory = media_directory.join("covers");
                        fs::create_dir_all(&cover_directory)
                            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
                        let cover_name = format!("cover_{}.png", asset.summary.id);
                        let mut cover_file = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(cover_directory.join(&cover_name))
                            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
                        cover_file
                            .write_all(&cover.bytes)
                            .and_then(|()| cover_file.flush())
                            .and_then(|()| cover_file.sync_all())
                            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
                        exported_covers.push(ExportedCover {
                            cover: cover.cover,
                            path: format!("media/covers/{cover_name}"),
                        });
                    }
                }
                asset.summary.media.path_kind = PathKind::Managed;
                asset.summary.media.stored_path = relative_path;
                asset.summary.media.file_size = Some(copied_size);
                write_pretty_array_item(&mut manifest, &asset, first_asset)?;
                first_asset = false;
                Ok(())
            },
        )?;
        let categories = library
            .list_categories(root, None)
            .map_err(ExportServiceError::Library)?
            .into_iter()
            .filter(|item| category_ids.contains(&item.id))
            .collect::<Vec<_>>();
        let dimension_ids = categories
            .iter()
            .map(|item| item.dimension_id)
            .collect::<HashSet<_>>();
        let dimensions = library
            .list_dimensions(root)
            .map_err(ExportServiceError::Library)?
            .into_iter()
            .filter(|item| dimension_ids.contains(&item.id))
            .collect::<Vec<_>>();
        let tags = library
            .list_tags(root)
            .map_err(ExportServiceError::Library)?
            .into_iter()
            .filter(|item| tag_ids.contains(&item.id))
            .collect::<Vec<_>>();
        write_pretty_array_end(&mut manifest, !first_asset)?;
        manifest
            .write_all(b",\n  \"dimensions\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &dimensions)?;
        manifest
            .write_all(b",\n  \"categories\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &categories)?;
        manifest
            .write_all(b",\n  \"tags\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &tags)?;
        manifest
            .write_all(b",\n  \"customFields\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &contents.custom_fields)?;
        manifest
            .write_all(b",\n  \"customFieldValues\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &contents.custom_field_values)?;
        manifest
            .write_all(b",\n  \"videoCovers\": ")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write_pretty_json_array(&mut manifest, &exported_covers)?;
        manifest
            .write_all(b",\n  \"canvasMembers\": [")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        let canvas_members_empty = write_canvas_members(
            library,
            root,
            &contents.canvas_project_ids,
            contents.canvas_member_asset_ids.as_ref(),
            &mut manifest,
            false,
        )?;
        manifest
            .write_all(if canvas_members_empty {
                b"]\n}\n"
            } else {
                b"\n  ]\n}\n"
            })
            .and_then(|()| manifest.flush())
            .and_then(|()| manifest.sync_all())
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        drop(manifest);
        fs::rename(&staging, &final_directory)
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        Ok(final_directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("export")
            .to_owned())
    })();

    if export_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    export_result
}

fn export_prompts(
    library: &LibraryService,
    root: &Path,
    target: &Path,
    contents: ExportContents,
) -> Result<String, ExportServiceError> {
    let output_path = unique_path(target, "作品提示词", "json");
    let temporary_path = output_path.with_extension("json.tmp");
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        write!(
            file,
            "{{\n  \"formatVersion\": 1,\n  \"exportedAt\": {},\n  \"projects\": ",
            current_time_millis()?
        )
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        let projects = contents
            .projects
            .into_iter()
            .map(|project| PromptProject {
                id: project.summary.id,
                title: project.summary.title,
                prompt: project.summary.prompt,
            })
            .collect::<Vec<_>>();
        write_pretty_json_array(&mut file, &projects)?;
        file.write_all(b",\n  \"assets\": [")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        let mut first_asset = true;
        for_each_asset(
            library,
            root,
            &contents.asset_ids,
            &contents.selected_assets,
            |asset| {
                let prompt_asset = PromptAsset {
                    id: asset.summary.id,
                    file_name: asset.summary.media.file_name,
                    project_id: asset.summary.media.project_id,
                    prompt: asset.summary.prompt,
                };
                write_pretty_array_item(&mut file, &prompt_asset, first_asset)?;
                first_asset = false;
                Ok(())
            },
        )?;
        write_pretty_array_end(&mut file, !first_asset)?;
        file.write_all(b",\n  \"canvasMembers\": [")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        let canvas_members_empty = write_canvas_members(
            library,
            root,
            &contents.canvas_project_ids,
            contents.canvas_member_asset_ids.as_ref(),
            &mut file,
            true,
        )?;
        file.write_all(if canvas_members_empty {
            b"]\n}\n"
        } else {
            b"\n  ]\n}\n"
        })
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        drop(file);
        fs::rename(&temporary_path, &output_path)
            .map_err(|_| ExportServiceError::DestinationUnavailable)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result?;
    Ok(output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prompts.json")
        .to_owned())
}

fn for_each_asset(
    library: &LibraryService,
    root: &Path,
    asset_ids: &[i64],
    selected_assets: &[AssetDetail],
    mut visit: impl FnMut(AssetDetail) -> Result<(), ExportServiceError>,
) -> Result<(), ExportServiceError> {
    if !selected_assets.is_empty() {
        for asset in selected_assets.iter().cloned() {
            visit(asset)?;
        }
        return Ok(());
    }
    for batch in asset_ids.chunks(PAGE_LIMIT as usize) {
        let assets = library
            .get_asset_details_for_export(root, batch)
            .map_err(ExportServiceError::Library)?;
        for asset in assets {
            visit(asset)?;
        }
    }
    Ok(())
}

fn write_canvas_members(
    library: &LibraryService,
    root: &Path,
    project_ids: &[i64],
    asset_ids: Option<&HashSet<i64>>,
    writer: &mut impl Write,
    prompts_only: bool,
) -> Result<bool, ExportServiceError> {
    let mut first_member = true;
    let mut member_count = 0;
    for project_id in project_ids {
        let mut cursor = None;
        loop {
            let page = library
                .list_canvas_members(root, *project_id, cursor, PAGE_LIMIT, None)
                .map_err(ExportServiceError::Library)?;
            for member in page.items {
                if asset_ids.is_some_and(|ids| !ids.contains(&member.asset_id)) {
                    continue;
                }
                member_count += 1;
                if member_count > MAX_EXPORTED_CANVAS_MEMBERS {
                    return Err(ExportServiceError::InvalidInput);
                }
                if prompts_only {
                    let value = PromptCanvasMember {
                        project_id: *project_id,
                        asset_id: member.asset_id,
                        role: member.role,
                        reference_name: member.reference_name,
                        prompt_zh: member.prompt_zh,
                        prompt_en: member.prompt_en,
                        negative_prompt: member.negative_prompt,
                    };
                    write_pretty_array_item(writer, &value, first_member)?;
                } else {
                    let value = ExportCanvasMember {
                        project_id: *project_id,
                        asset_id: member.asset_id,
                        display_order: member.display_order,
                        file_name: member.file_name,
                        media_type: member.media_type,
                        model_name: member.model_name,
                        platform_name: member.platform_name,
                        width: member.width,
                        height: member.height,
                        duration_ms: member.duration_ms,
                        updated_at: member.updated_at,
                        role: member.role,
                        reference_name: member.reference_name,
                        prompt_zh: member.prompt_zh,
                        prompt_en: member.prompt_en,
                        negative_prompt: member.negative_prompt,
                    };
                    write_pretty_array_item(writer, &value, first_member)?;
                }
                first_member = false;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(first_member)
}

fn write_pretty_json_array<T: Serialize>(
    writer: &mut impl Write,
    values: &[T],
) -> Result<(), ExportServiceError> {
    if values.is_empty() {
        writer
            .write_all(b"[]")
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        return Ok(());
    }
    writer
        .write_all(b"[")
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
    for (index, value) in values.iter().enumerate() {
        write_pretty_array_item(writer, value, index == 0)?;
    }
    writer
        .write_all(b"\n  ]")
        .map_err(|_| ExportServiceError::DestinationUnavailable)
}

fn write_pretty_array_item<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
    is_first: bool,
) -> Result<(), ExportServiceError> {
    writer
        .write_all(if is_first { b"\n" } else { b",\n" })
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
    let encoded =
        serde_json::to_vec_pretty(value).map_err(|_| ExportServiceError::DestinationUnavailable)?;
    for (index, line) in encoded.split(|byte| *byte == b'\n').enumerate() {
        if index > 0 {
            writer
                .write_all(b"\n")
                .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        }
        writer
            .write_all(b"    ")
            .and_then(|()| writer.write_all(line))
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
    }
    Ok(())
}

fn write_pretty_array_end(
    writer: &mut impl Write,
    has_items: bool,
) -> Result<(), ExportServiceError> {
    writer
        .write_all(if has_items { b"\n  ]" } else { b"]" })
        .map_err(|_| ExportServiceError::DestinationUnavailable)
}

fn validate_ids(ids: &[i64]) -> Result<(), ExportServiceError> {
    if ids.is_empty()
        || ids.len() > MAX_SELECTED_ENTITIES
        || ids.iter().any(|id| *id <= 0)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(ExportServiceError::InvalidInput);
    }
    Ok(())
}

fn validate_target_directory(root: &Path, target: &Path) -> Result<PathBuf, ExportServiceError> {
    let target =
        fs::canonicalize(target).map_err(|_| ExportServiceError::DestinationUnavailable)?;
    if !target.is_dir() {
        return Err(ExportServiceError::DestinationUnavailable);
    }
    if target.starts_with(root) {
        let exports_root = fs::canonicalize(root.join("exports"))
            .map_err(|_| ExportServiceError::DestinationUnavailable)?;
        if !target.starts_with(exports_root) {
            return Err(ExportServiceError::DestinationUnavailable);
        }
    }
    Ok(target)
}

fn create_export_directory(target: &Path) -> Result<(PathBuf, PathBuf), ExportServiceError> {
    for _ in 0..100 {
        let token = unique_token();
        let staging = target.join(format!(".aigallery-export-{token}.tmp"));
        let final_directory = target.join(format!("作品导出-{token}"));
        if final_directory.exists() {
            continue;
        }
        match fs::create_dir(&staging) {
            Ok(()) => return Ok((staging, final_directory)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ExportServiceError::DestinationUnavailable),
        }
    }
    Err(ExportServiceError::DestinationUnavailable)
}

fn unique_path(parent: &Path, prefix: &str, extension: &str) -> PathBuf {
    parent.join(format!("{prefix}-{}.{}", unique_token(), extension))
}

fn unique_token() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = EXPORT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp}-{}-{counter}", std::process::id())
}

fn resolve_source_path(
    workspace: &WorkspaceService,
    root: &Path,
    asset: &AssetDetail,
) -> Result<PathBuf, ExportServiceError> {
    let stored_path = Path::new(&asset.summary.media.stored_path);
    let kind = match asset.summary.media.path_kind {
        PathKind::Managed => StoredPathKind::Managed,
        PathKind::External => StoredPathKind::External,
    };
    let status = workspace
        .check_stored_path(root, kind, stored_path)
        .map_err(ExportServiceError::Workspace)?;
    if status.availability != PathAvailability::Available {
        return Err(ExportServiceError::SourceUnavailable);
    }
    let source = match kind {
        StoredPathKind::Managed => root.join(stored_path),
        StoredPathKind::External => stored_path.to_path_buf(),
    };
    Ok(source)
}

fn copy_media_file(
    root: &Path,
    source: &Path,
    destination: &Path,
    asset: &AssetDetail,
) -> Result<i64, ExportServiceError> {
    let mut source_file = File::open(source).map_err(|_| ExportServiceError::SourceUnavailable)?;
    if asset.summary.media.path_kind == PathKind::Managed {
        let trusted_root = root
            .canonicalize()
            .map_err(|_| ExportServiceError::SourceUnavailable)?;
        super::ensure_handle_inside_root(&source_file, &trusted_root)
            .map_err(|_| ExportServiceError::SourceUnavailable)?;
    }
    let source_metadata = source_file
        .metadata()
        .map_err(|_| ExportServiceError::SourceUnavailable)?;
    if !source_metadata.is_file() {
        return Err(ExportServiceError::SourceUnavailable);
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
    io::copy(&mut source_file, &mut output)
        .and_then(|_| output.flush())
        .and_then(|_| output.sync_all())
        .map_err(|_| ExportServiceError::DestinationUnavailable)?;
    i64::try_from(
        output
            .metadata()
            .map_err(|_| ExportServiceError::DestinationUnavailable)?
            .len(),
    )
    .map_err(|_| ExportServiceError::DestinationUnavailable)
}

fn safe_file_name(file_name: &str) -> String {
    let base = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("media");
    let sanitized = base
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_matches(|character| character == ' ' || character == '.')
        .chars()
        .take(160)
        .collect::<String>();
    if sanitized.is_empty() {
        "media".to_owned()
    } else {
        sanitized
    }
}

fn current_time_millis() -> Result<i64, ExportServiceError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ExportServiceError::DestinationUnavailable)?
        .as_millis();
    i64::try_from(millis).map_err(|_| ExportServiceError::DestinationUnavailable)
}
