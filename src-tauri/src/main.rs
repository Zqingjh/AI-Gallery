#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager as _, path::BaseDirectory};

mod adapters;
mod commands;
mod domain;
mod repositories;
mod services;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::library::ThumbnailCommandState::default())
        .manage(commands::media_integrity::MediaIntegrityCommandState::default())
        .setup(|app| {
            let ffmpeg_path = app
                .path()
                .resolve("binaries/ffmpeg.exe", BaseDirectory::Resource)?;
            app.manage(commands::import::ImportCommandState::with_bundled_ffmpeg(
                ffmpeg_path,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ai::ai_list_providers,
            commands::ai::ai_save_provider,
            commands::ai::ai_delete_provider,
            commands::ai::ai_test_provider_connection,
            commands::ai::ai_get_send_preview,
            commands::ai::ai_get_batch_send_preview,
            commands::ai::ai_create_classification_suggestions,
            commands::ai::ai_create_classification_suggestions_batch,
            commands::ai::ai_list_pending_suggestions,
            commands::ai::ai_get_suggestion_impact,
            commands::ai::ai_resolve_suggestion,
            commands::backup::create_workspace_backup,
            commands::backup::restore_workspace_backup,
            commands::database::prepare_workspace_database,
            commands::import::start_media_import,
            commands::import::get_media_import_task,
            commands::import::cancel_media_import,
            commands::library::library_list_projects,
            commands::library::library_get_project,
            commands::library::library_create_project,
            commands::library::library_update_project,
            commands::library::library_assign_assets_to_project,
            commands::library::library_remove_assets_from_project,
            commands::library::library_list_canvas_members,
            commands::library::library_set_canvas_member,
            commands::library::library_update_canvas_output_prompt,
            commands::library::library_list_assets,
            commands::library::library_list_assets_numbered,
            commands::library::library_preview_bulk_asset_edit,
            commands::library::library_bulk_edit_assets,
            commands::library::library_list_model_comparison,
            commands::library::library_list_duplicate_groups,
            commands::library::library_get_asset,
            commands::library::library_read_asset_preview,
            commands::library::library_read_asset_thumbnail,
            commands::library::library_export_selection,
            commands::library::library_create_asset,
            commands::library::library_update_asset,
            commands::library::library_update_asset_display_order,
            commands::library::library_set_project_categories,
            commands::library::library_set_project_tags,
            commands::library::library_set_asset_categories,
            commands::library::library_set_asset_tags,
            commands::library::library_list_metadata_presets,
            commands::library::library_write_metadata_preset,
            commands::library::library_delete_metadata_preset,
            commands::library::library_list_dimensions,
            commands::library::library_write_dimension,
            commands::library::library_delete_dimension,
            commands::library::library_list_categories,
            commands::library::library_write_category,
            commands::library::library_category_impact,
            commands::library::library_delete_category,
            commands::library::library_list_tags,
            commands::library::library_write_tag,
            commands::library::library_delete_tag,
            commands::library::library_move_project_to_trash,
            commands::library::library_move_asset_to_trash,
            commands::library::library_move_assets_to_trash,
            commands::library::library_list_trash,
            commands::library::library_restore_trash,
            commands::library::library_restore_trash_batch,
            commands::library::library_undo_recent_delete,
            commands::library::library_purge_trash_record,
            commands::library::library_purge_trash_records,
            commands::media_integrity::get_asset_cover,
            commands::media_integrity::preview_video_key_frame,
            commands::media_integrity::set_video_key_frame_cover,
            commands::media_integrity::set_default_video_cover,
            commands::media_integrity::set_custom_video_cover,
            commands::media_integrity::remove_video_cover,
            commands::media_integrity::preview_asset_path_repair,
            commands::media_integrity::execute_asset_path_repair,
            commands::p1_library::p1_library_save_saved_filter,
            commands::p1_library::p1_library_list_saved_filters,
            commands::p1_library::p1_library_delete_saved_filter,
            commands::p1_library::p1_library_save_custom_field,
            commands::p1_library::p1_library_list_custom_fields,
            commands::p1_library::p1_library_save_manual_custom_field_value,
            commands::p1_library::p1_library_confirm_pending_custom_field_value,
            commands::p1_library::p1_library_list_custom_field_values,
            commands::p1_library::p1_library_create_prompt_version,
            commands::p1_library::p1_library_list_prompt_versions,
            commands::p1_library::p1_library_list_edit_history,
            commands::runtime::get_runtime_info,
            commands::workspace::create_workspace,
            commands::workspace::open_workspace,
            commands::workspace::validate_isolated_workspace,
            commands::workspace::get_workspace_access_mode,
            commands::workspace::set_workspace_access_mode,
            commands::workspace::check_stored_path
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 AI Gallery 桌面应用");
}
