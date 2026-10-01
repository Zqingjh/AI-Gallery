import { useEffect, useState } from "react";
import { text } from "../../app/texts";
import { ThemeToggle } from "../../components/ThemeToggle";
import { LibraryDialog } from "../library/LibraryDialog";
import type {
  AiProvider,
  AiProviderKind,
  AiService,
  SaveAiProviderInput,
} from "../../services/ai-service";
import type {
  WorkspaceAccessMode,
  WorkspaceBackupKind,
  WorkspaceManagementService,
} from "../../services/workspace-management-service";
import { ServiceError } from "../../services/service-error";

interface SettingsPageProps {
  readonly aiService?: AiService;
  readonly workspaceManagementService?: WorkspaceManagementService;
  readonly workspaceRoot: string | null;
  readonly accessMode: WorkspaceAccessMode;
  readonly onAccessModeChanged: (mode: WorkspaceAccessMode) => void;
  readonly onBack: () => void;
  readonly showThemeToggle?: boolean;
}

interface ProviderFormState {
  readonly id: string | null;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  readonly endpoint: string;
  readonly model: string;
  readonly timeoutMs: string;
  readonly isEnabled: boolean;
  readonly textClassification: boolean;
  readonly vision: boolean;
}

const emptyProvider: ProviderFormState = {
  id: null,
  kind: "openaiCompatible",
  displayName: "",
  endpoint: "",
  model: "",
  timeoutMs: "30000",
  isEnabled: true,
  textClassification: true,
  vision: false,
};

const providerPresets: ReadonlyArray<{
  readonly id: string;
  readonly label: string;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  readonly endpoint: string;
  readonly model: string;
}> = [
  {
    id: "custom",
    label: "自定义",
    kind: "openaiCompatible",
    displayName: "",
    endpoint: "",
    model: "",
  },
  {
    id: "openai",
    label: "OpenAI",
    kind: "openaiCompatible",
    displayName: "OpenAI",
    endpoint: "https://api.openai.com/v1",
    model: "gpt-5.2",
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    kind: "openaiCompatible",
    displayName: "DeepSeek",
    endpoint: "https://api.deepseek.com",
    model: "deepseek-v4-flash",
  },
  {
    id: "qwen",
    label: "通义千问（百炼）",
    kind: "openaiCompatible",
    displayName: "通义千问",
    endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
  },
  {
    id: "zhipu",
    label: "智谱 AI",
    kind: "openaiCompatible",
    displayName: "智谱 AI",
    endpoint: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-5.2",
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    kind: "openaiCompatible",
    displayName: "OpenRouter",
    endpoint: "https://openrouter.ai/api/v1",
    model: "openai/gpt-5.2",
  },
  {
    id: "gemini",
    label: "Google Gemini",
    kind: "gemini",
    displayName: "Google Gemini",
    endpoint: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.5-flash",
  },
  {
    id: "ollama",
    label: "Ollama（本机）",
    kind: "ollama",
    displayName: "Ollama",
    endpoint: "http://127.0.0.1:11434",
    model: "llama3.2",
  },
];

function editState(provider: AiProvider): ProviderFormState {
  return {
    id: provider.id,
    kind: provider.kind,
    displayName: provider.displayName,
    endpoint: provider.endpoint,
    model: provider.model,
    timeoutMs: String(provider.timeoutMs),
    isEnabled: provider.isEnabled,
    textClassification: provider.capabilities.textClassification,
    vision: provider.capabilities.vision,
  };
}

function providerInput(
  form: ProviderFormState,
  formData: FormData,
): SaveAiProviderInput | null {
  const timeoutMs = Number(form.timeoutMs);
  if (
    !form.displayName.trim() ||
    !form.endpoint.trim() ||
    !form.model.trim() ||
    !Number.isInteger(timeoutMs) ||
    timeoutMs < 1_000 ||
    timeoutMs > 60_000
  ) {
    return null;
  }
  const apiKey = formData.get("api-key");
  return {
    id: form.id,
    kind: form.kind,
    displayName: form.displayName.trim(),
    endpoint: form.endpoint.trim(),
    model: form.model.trim(),
    capabilities: {
      textClassification: form.textClassification,
      vision: form.vision,
    },
    timeoutMs,
    isEnabled: form.isEnabled,
    ...(typeof apiKey === "string" && apiKey ? { apiKey } : {}),
  };
}

function safeOperationError(error: unknown, fallback: string): string {
  return error instanceof ServiceError ? error.message : fallback;
}

function newRestoreTarget(parentPath: string, now = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  const suffix = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  const separator = parentPath.includes("\\") ? "\\" : "/";
  const parent = parentPath.replace(/[\\/]+$/, "");
  return `${parent}${separator}AI-Gallery-Restored-${suffix}`;
}

export default function SettingsPage({
  aiService,
  workspaceManagementService,
  workspaceRoot,
  accessMode,
  onAccessModeChanged,
  onBack,
  showThemeToggle = true,
}: SettingsPageProps) {
  const [providers, setProviders] = useState<readonly AiProvider[]>([]);
  const [form, setForm] = useState<ProviderFormState>(emptyProvider);
  const [providerDialogOpen, setProviderDialogOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [backupKind, setBackupKind] = useState<WorkspaceBackupKind>("full");
  const [backupDestination, setBackupDestination] = useState("");
  const [backupSource, setBackupSource] = useState("");
  const [restoreTarget, setRestoreTarget] = useState("");
  const [workspaceBusy, setWorkspaceBusy] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);
  const [workspaceNotice, setWorkspaceNotice] = useState<string | null>(null);
  const isReadOnly = accessMode === "readOnly";
  const canManageAi = Boolean(aiService && workspaceRoot) && !isReadOnly;

  async function loadProviders() {
    if (!canManageAi || !aiService) return;
    setError(null);
    try {
      setProviders(await aiService.listProviders());
    } catch (error: unknown) {
      setError(safeOperationError(error, text.ai.loadFailed));
    }
  }

  useEffect(() => {
    void loadProviders();
  }, [aiService, canManageAi]);

  async function createBackup() {
    if (!workspaceManagementService || !workspaceRoot) {
      setWorkspaceError(text.workspace.unavailable);
      return;
    }
    if (!backupDestination.trim()) {
      setWorkspaceError(text.workspace.backupDestinationRequired);
      return;
    }
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    setWorkspaceNotice(null);
    try {
      const result = await workspaceManagementService.createBackup({
        rootPath: workspaceRoot,
        destinationParentPath: backupDestination.trim(),
        kind: backupKind,
      });
      setWorkspaceNotice(
        text.workspace.backupCreated(
          result.backupName,
          result.managedAssetCount,
          result.externalAssetCount,
        ),
      );
    } catch (error: unknown) {
      setWorkspaceError(
        safeOperationError(error, text.workspace.operationFailed),
      );
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function restoreBackup() {
    if (!workspaceManagementService) {
      setWorkspaceError(text.workspace.unavailable);
      return;
    }
    if (!backupSource.trim()) {
      setWorkspaceError(text.workspace.backupSourceRequired);
      return;
    }
    if (!restoreTarget.trim()) {
      setWorkspaceError(text.workspace.restoreTargetRequired);
      return;
    }
    if (!window.confirm(text.workspace.restoreConfirm)) return;
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    setWorkspaceNotice(null);
    try {
      await workspaceManagementService.restoreBackup({
        backupRootPath: backupSource.trim(),
        targetRootPath: restoreTarget.trim(),
        confirmed: true,
      });
      setWorkspaceNotice(text.workspace.restored);
    } catch (error: unknown) {
      setWorkspaceError(
        safeOperationError(error, text.workspace.operationFailed),
      );
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function updateAccessMode() {
    if (!workspaceManagementService || !workspaceRoot) {
      setWorkspaceError(text.workspace.unavailable);
      return;
    }
    const nextMode: WorkspaceAccessMode = isReadOnly ? "readWrite" : "readOnly";
    if (
      nextMode === "readWrite" &&
      !window.confirm(text.workspace.disableReadOnlyConfirm)
    ) {
      return;
    }
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    try {
      const mode = await workspaceManagementService.setAccessMode({
        rootPath: workspaceRoot,
        mode: nextMode,
        confirmed: true,
      });
      onAccessModeChanged(mode);
      setWorkspaceNotice(text.workspace.modeUpdated);
    } catch (error: unknown) {
      setWorkspaceError(
        safeOperationError(error, text.workspace.operationFailed),
      );
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function selectDirectory(applySelection: (selected: string) => void) {
    if (!workspaceManagementService) {
      setWorkspaceError(text.workspace.unavailable);
      return;
    }
    setWorkspaceError(null);
    try {
      const selected = await workspaceManagementService.selectDirectory();
      if (selected) applySelection(selected);
    } catch (error: unknown) {
      setWorkspaceError(
        safeOperationError(error, text.workspace.directoryPickerFailed),
      );
    }
  }

  async function submitProvider(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    if (!aiService) {
      setError(text.ai.noWorkspace);
      return;
    }
    const input = providerInput(form, new FormData(formElement));
    if (!input) {
      setError(text.ai.invalidProviderForm);
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await aiService.saveProvider(input);
      formElement.reset();
      setForm(emptyProvider);
      setProviderDialogOpen(false);
      await loadProviders();
      setNotice(text.ai.providerSaved);
    } catch (error: unknown) {
      setError(safeOperationError(error, text.ai.operationFailed));
    } finally {
      setBusy(false);
    }
  }

  async function deleteProvider(id: string) {
    if (!aiService || !window.confirm(text.ai.confirmDeleteProvider)) return;
    setBusy(true);
    setError(null);
    try {
      await aiService.deleteProvider(id);
      if (form.id === id) {
        setForm(emptyProvider);
        setProviderDialogOpen(false);
      }
      await loadProviders();
      setNotice(text.ai.providerDeleted);
    } catch (error: unknown) {
      setError(safeOperationError(error, text.ai.operationFailed));
    } finally {
      setBusy(false);
    }
  }

  async function testProvider(id: string) {
    if (!aiService) return;
    setTestingId(id);
    setError(null);
    setNotice(null);
    try {
      await aiService.testProvider(id);
      setNotice(text.ai.testSucceeded);
    } catch (error: unknown) {
      setError(safeOperationError(error, text.ai.operationFailed));
    } finally {
      setTestingId(null);
    }
  }

  return (
    <main className="settings-page settings-management-page">
      <div className="settings-toolbar">
        <button className="secondary-button" type="button" onClick={onBack}>
          ← {text.settings.back}
        </button>
        {showThemeToggle ? <ThemeToggle /> : null}
      </div>
      <section className="settings-panel settings-introduction">
        <p className="eyebrow">{text.settings.eyebrow}</p>
        <h1>{text.settings.title}</h1>
        <p>{text.settings.description}</p>
      </section>
      {notice || error ? (
        <div className="settings-feedback" aria-live="polite">
          {notice ? <p role="status">{notice}</p> : null}
          {error ? (
            <p className="form-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      ) : null}

      <section
        className="settings-panel management-panel"
        aria-labelledby="workspace-management-title"
      >
        <header>
          <div>
            <p className="eyebrow">{text.workspace.eyebrow}</p>
            <h2 id="workspace-management-title">{text.workspace.title}</h2>
          </div>
        </header>
        <p>{text.workspace.description}</p>
        {!workspaceRoot || !workspaceManagementService ? (
          <p className="form-error" role="alert">
            {text.workspace.unavailable}
          </p>
        ) : (
          <>
            <div className="workspace-management-grid">
              <label className="form-field">
                <span>{text.workspace.backupKind}</span>
                <select
                  value={backupKind}
                  onChange={(event) =>
                    setBackupKind(event.target.value as WorkspaceBackupKind)
                  }
                >
                  <option value="full">{text.workspace.fullBackup}</option>
                  <option value="light">{text.workspace.lightBackup}</option>
                </select>
                <small>
                  {backupKind === "full"
                    ? text.workspace.fullBackupHint
                    : text.workspace.lightBackupHint}
                </small>
              </label>
              <div className="form-field path-picker-field">
                <label htmlFor="backup-destination">
                  {text.workspace.backupDestination}
                </label>
                <div className="path-picker-row">
                  <input
                    id="backup-destination"
                    value={backupDestination}
                    placeholder={text.workspace.backupDestinationPlaceholder}
                    onChange={(event) =>
                      setBackupDestination(event.target.value)
                    }
                  />
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={workspaceBusy}
                    onClick={() => void selectDirectory(setBackupDestination)}
                  >
                    {text.workspace.selectBackupDestination}
                  </button>
                </div>
              </div>
              <button
                className="secondary-button"
                type="button"
                disabled={workspaceBusy}
                onClick={() => void createBackup()}
              >
                {workspaceBusy
                  ? text.workspace.creatingBackup
                  : text.workspace.createBackup}
              </button>
            </div>
            <div className="workspace-management-grid restore-panel">
              <h3>{text.workspace.restoreTitle}</h3>
              <div className="form-field path-picker-field">
                <label htmlFor="backup-source">
                  {text.workspace.backupRoot}
                </label>
                <div className="path-picker-row">
                  <input
                    id="backup-source"
                    value={backupSource}
                    placeholder={text.workspace.backupRootPlaceholder}
                    onChange={(event) => setBackupSource(event.target.value)}
                  />
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={workspaceBusy}
                    onClick={() => void selectDirectory(setBackupSource)}
                  >
                    {text.workspace.selectBackupSource}
                  </button>
                </div>
              </div>
              <div className="form-field path-picker-field">
                <label htmlFor="restore-target">
                  {text.workspace.restoreTarget}
                </label>
                <div className="path-picker-row">
                  <input
                    id="restore-target"
                    value={restoreTarget}
                    placeholder={text.workspace.restoreTargetPlaceholder}
                    onChange={(event) => setRestoreTarget(event.target.value)}
                  />
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={workspaceBusy}
                    onClick={() =>
                      void selectDirectory((parent) =>
                        setRestoreTarget(newRestoreTarget(parent)),
                      )
                    }
                  >
                    {text.workspace.selectRestoreTarget}
                  </button>
                </div>
                <small>{text.workspace.restoreTargetHint}</small>
              </div>
              <button
                className="danger-text-button"
                type="button"
                disabled={workspaceBusy}
                onClick={() => void restoreBackup()}
              >
                {workspaceBusy
                  ? text.workspace.restoring
                  : text.workspace.restore}
              </button>
            </div>
            <div className="workspace-access-row">
              <div>
                <h3>{text.workspace.accessModeTitle}</h3>
                <p>
                  {isReadOnly
                    ? text.workspace.readOnlyDescription
                    : text.workspace.readWriteDescription}
                </p>
              </div>
              <button
                className={isReadOnly ? "primary-button" : "secondary-button"}
                type="button"
                disabled={workspaceBusy}
                onClick={() => void updateAccessMode()}
              >
                {isReadOnly
                  ? text.workspace.disableReadOnly
                  : text.workspace.enableReadOnly}
              </button>
            </div>
          </>
        )}
        {workspaceNotice ? <p role="status">{workspaceNotice}</p> : null}
        {workspaceError ? (
          <p className="form-error" role="alert">
            {workspaceError}
          </p>
        ) : null}
      </section>

      {canManageAi ? (
        <>
          <section
            className="settings-panel management-panel"
            aria-labelledby="ai-provider-list-title"
          >
            <header>
              <div>
                <p className="eyebrow">{text.ai.settingsEyebrow}</p>
                <h2 id="ai-provider-list-title">{text.ai.providerList}</h2>
              </div>
              <button
                className="secondary-button"
                type="button"
                onClick={() => {
                  setForm(emptyProvider);
                  setProviderDialogOpen(true);
                }}
              >
                {text.ai.addProvider}
              </button>
            </header>
            {providers.length === 0 ? (
              <p className="management-empty">{text.ai.providerEmpty}</p>
            ) : null}
            <div className="project-grid ai-provider-grid">
              {providers.map((provider) => (
                <article
                  className="project-card ai-provider-card"
                  key={provider.id}
                >
                  <button
                    className="danger-text-button ai-provider-delete"
                    type="button"
                    disabled={busy}
                    onClick={() => void deleteProvider(provider.id)}
                  >
                    {text.ai.deleteProvider}
                  </button>
                  <p className="eyebrow">{provider.kind}</p>
                  <h2>{provider.displayName}</h2>
                  <p>{provider.model}</p>
                  <small>
                    {provider.needsCredential
                      ? text.ai.providerNeedsCredential
                      : text.ai.providerReady}
                  </small>
                  <footer>
                    <button
                      className="text-button"
                      type="button"
                      onClick={() => {
                        setForm(editState(provider));
                        setProviderDialogOpen(true);
                      }}
                    >
                      {text.ai.editProvider}
                    </button>
                    <button
                      className="text-button"
                      type="button"
                      disabled={testingId === provider.id}
                      onClick={() => void testProvider(provider.id)}
                    >
                      {testingId === provider.id
                        ? text.ai.testingProvider
                        : text.ai.testProvider}
                    </button>
                  </footer>
                </article>
              ))}
            </div>
          </section>

          {providerDialogOpen ? (
            <LibraryDialog
              title={form.id ? "编辑 AI 服务" : text.ai.providerFormTitle}
              description="服务地址可由预设自动填入；密钥仅在本次保存时提交。"
              onClose={() => {
                setForm(emptyProvider);
                setProviderDialogOpen(false);
              }}
            >
              <form
                className="library-form provider-form"
                onSubmit={(event) => void submitProvider(event)}
              >
                <label className="form-field">
                  <span>服务预设</span>
                  <select
                    defaultValue="custom"
                    onChange={(event) => {
                      const preset = providerPresets.find(
                        (item) => item.id === event.target.value,
                      );
                      if (!preset || preset.id === "custom") return;
                      setForm({
                        ...form,
                        kind: preset.kind,
                        displayName: preset.displayName,
                        endpoint: preset.endpoint,
                        model: preset.model,
                      });
                    }}
                  >
                    {providerPresets.map((preset) => (
                      <option key={preset.id} value={preset.id}>
                        {preset.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="form-field">
                  <span>{text.ai.providerName}</span>
                  <input
                    required
                    value={form.displayName}
                    onChange={(event) =>
                      setForm({ ...form, displayName: event.target.value })
                    }
                  />
                </label>
                <label className="form-field">
                  <span>{text.ai.providerEndpoint}</span>
                  <input
                    required
                    type="url"
                    value={form.endpoint}
                    onChange={(event) =>
                      setForm({ ...form, endpoint: event.target.value })
                    }
                  />
                </label>
                <label className="form-field">
                  <span>{text.ai.providerModel}</span>
                  <input
                    required
                    value={form.model}
                    onChange={(event) =>
                      setForm({ ...form, model: event.target.value })
                    }
                  />
                </label>
                <label className="form-field">
                  <span>{text.ai.providerTimeout}</span>
                  <input
                    required
                    type="number"
                    min="1000"
                    max="60000"
                    value={form.timeoutMs}
                    onChange={(event) =>
                      setForm({ ...form, timeoutMs: event.target.value })
                    }
                  />
                </label>
                <label className="form-field">
                  <span>{text.ai.providerKey}</span>
                  <input name="api-key" autoComplete="off" type="password" />
                  <small>{text.ai.providerKeyHint}</small>
                </label>
                <fieldset className="form-flags">
                  <label className="check-field">
                    <input
                      type="checkbox"
                      checked={form.isEnabled}
                      onChange={(event) =>
                        setForm({ ...form, isEnabled: event.target.checked })
                      }
                    />
                    {text.ai.providerEnabled}
                  </label>
                  <label className="check-field">
                    <input
                      type="checkbox"
                      checked={form.textClassification}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          textClassification: event.target.checked,
                        })
                      }
                    />
                    {text.ai.providerTextCapability}
                  </label>
                  <label className="check-field">
                    <input
                      type="checkbox"
                      checked={form.vision}
                      onChange={(event) =>
                        setForm({ ...form, vision: event.target.checked })
                      }
                    />
                    {text.ai.providerVisionCapability}
                  </label>
                </fieldset>
                <div className="dialog-actions">
                  <button
                    className="secondary-button"
                    type="button"
                    onClick={() => {
                      setForm(emptyProvider);
                      setProviderDialogOpen(false);
                    }}
                  >
                    {text.ai.cancelEdit}
                  </button>
                  <button
                    className="primary-button"
                    type="submit"
                    disabled={busy}
                  >
                    {busy ? text.ai.savingProvider : text.ai.saveProvider}
                  </button>
                </div>
              </form>
            </LibraryDialog>
          ) : null}
        </>
      ) : (
        <section className="settings-panel management-panel">
          <p className="management-empty" role="alert">
            {isReadOnly
              ? text.workspace.readOnlyDescription
              : text.ai.noWorkspace}
          </p>
        </section>
      )}
    </main>
  );
}
