export const serviceErrorCode = {
  commandFailed: "COMMAND_FAILED",
  invalidResponse: "INVALID_RESPONSE",
} as const;

interface ErrorDto {
  readonly code: string;
  readonly message: string;
}

const commandErrorMessages = {
  RUNTIME_INFO_UNAVAILABLE: "无法获取运行环境信息。",
  INVALID_WORKSPACE_PATH: "请选择有效的工作区位置。",
  WORKSPACE_ALREADY_EXISTS: "所选位置已存在，无法创建工作区。",
  WORKSPACE_NOT_FOUND: "未找到工作区。",
  WORKSPACE_MANIFEST_MISSING: "工作区标记文件缺失。",
  WORKSPACE_MANIFEST_TOO_LARGE: "工作区标记文件超出大小限制。",
  WORKSPACE_MANIFEST_INVALID: "工作区标记文件无法识别。",
  WORKSPACE_VERSION_UNSUPPORTED: "当前版本无法打开此工作区。",
  WORKSPACE_STRUCTURE_INVALID: "工作区目录结构不完整。",
  WORKSPACE_CREATE_FAILED: "工作区创建失败。",
  WORKSPACE_OPEN_FAILED: "工作区打开失败。",
  WORKSPACE_READ_ONLY: "工作区正处于只读展示模式，无法修改内容。",
  WORKSPACE_PATH_CONFLICT: "私密工作区必须与正常工作区使用互不嵌套的独立目录。",
  BACKUP_INVALID_INPUT: "备份或恢复位置无效。",
  BACKUP_FAILED: "无法完成工作区备份。",
  BACKUP_RESTORE_FAILED: "无法完成工作区恢复，目标内容未被覆盖。",
  BACKUP_CONFLICT: "目标位置已存在或与现有内容冲突。",
  WORKSPACE_ROLLBACK_FAILED: "工作区创建失败，且未能完整清理临时内容。",
  DATABASE_PATH_UNSAFE: "数据库位置不安全，无法打开工作区。",
  DATABASE_OPEN_FAILED: "无法打开工作区数据库。",
  DATABASE_BACKUP_FAILED: "数据库迁移前备份失败，未执行迁移。",
  DATABASE_MIGRATION_FAILED: "数据库升级失败，已撤销本次结构变更。",
  DATABASE_VERSION_UNSUPPORTED: "当前版本无法打开此数据库。",
  DATABASE_SCHEMA_INVALID: "数据库结构不完整或已损坏。",
  LIBRARY_INVALID_INPUT: "提交的作品库数据无法识别。",
  LIBRARY_NOT_FOUND: "未找到指定的作品库记录。",
  LIBRARY_CONFLICT: "该名称或媒体记录已存在。",
  LIBRARY_DATABASE_UNAVAILABLE: "作品库数据库暂时不可用。",
  LIBRARY_DATA_INVALID: "作品库数据不完整或已损坏。",
  P1_LIBRARY_INVALID_INPUT: "提交的作品库增强数据无法识别。",
  P1_LIBRARY_NOT_FOUND: "未找到指定的作品库增强记录。",
  P1_LIBRARY_CONFLICT: "当前作品库增强数据无法按该方式保存。",
  P1_LIBRARY_DATABASE_UNAVAILABLE: "作品库增强数据暂时不可用。",
  P1_LIBRARY_DATA_INVALID: "作品库增强数据不完整或已损坏。",
  IMPORT_INVALID_INPUT: "请选择受支持的图片或 MP4 文件。",
  IMPORT_CANCELLED: "导入已取消。",
  IMPORT_COMPENSATION_FAILED: "导入失败，且部分新复制文件未能自动清理。",
  IMPORT_FAILED: "无法完成媒体导入。",
  IMPORT_QUEUE_FULL: "当前导入任务过多，请稍后重试。",
  IMPORT_TASK_NOT_FOUND: "未找到导入任务。",
  MEDIA_PREVIEW_UNAVAILABLE: "无法读取媒体预览。",
  MEDIA_PREVIEW_TOO_LARGE: "媒体文件过大，无法在详情中直接预览。",
} as const;

type CommandErrorCode = keyof typeof commandErrorMessages;

export class ServiceError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "ServiceError";
    this.code = code;
  }
}

function isErrorDto(value: unknown): value is ErrorDto {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  return (
    "code" in value &&
    typeof value.code === "string" &&
    value.code.length > 0 &&
    "message" in value &&
    typeof value.message === "string" &&
    value.message.length > 0
  );
}

function isCommandErrorCode(code: string): code is CommandErrorCode {
  return Object.hasOwn(commandErrorMessages, code);
}

/**
 * IPC 可能拒绝任意 JavaScript 值。这里只向 UI 暴露稳定错误，避免泄露底层路径或调试信息。
 */
export function toServiceError(error: unknown): ServiceError {
  if (error instanceof ServiceError) {
    return error;
  }

  if (isErrorDto(error) && isCommandErrorCode(error.code)) {
    // 远端 message 只用于验证 DTO 形状；用户消息必须来自本地白名单，防止泄露内部路径。
    return new ServiceError(error.code, commandErrorMessages[error.code]);
  }

  return new ServiceError(
    serviceErrorCode.commandFailed,
    "桌面服务暂时不可用，请稍后重试。",
  );
}
