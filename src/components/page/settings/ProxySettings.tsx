import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Globe } from "lucide-solid";
import Card from "../../common/Card";
import { useI18n } from "../../../i18n";
import { getErrorMessage } from "../../../utils/errors";

interface ProxyConfig {
    mode: string;
    url: string;
}

export default function ProxySettings() {
    const { t } = useI18n();
    const [mode, setMode] = createSignal<string>("system");
    const [customUrl, setCustomUrl] = createSignal("");
    const [systemProxy, setSystemProxy] = createSignal<string | null>(null);
    const [isLoading, setIsLoading] = createSignal(true);
    const [isSaving, setIsSaving] = createSignal(false);
    const [isTesting, setIsTesting] = createSignal(false);
    const [error, setError] = createSignal<string | null>(null);
    const [saved, setSaved] = createSignal(false);
    const [testResult, setTestResult] = createSignal<string | null>(null);
    const [testError, setTestError] = createSignal<string | null>(null);
    const modeId = "settings-proxy-mode";
    const inputId = "settings-proxy-url";
    const statusId = "settings-proxy-status";
    const testStatusId = "settings-proxy-test-status";
    let savedTimeout: number | undefined;
    let testTimeout: number | undefined;

    const fetchConfig = async () => {
        setIsLoading(true);
        setError(null);
        try {
            const config = await invoke<ProxyConfig>("get_proxy_config");
            setMode(config.mode);
            setCustomUrl(config.url);
        } catch (err) {
            console.error("Failed to load proxy config:", getErrorMessage(err));
            setError(t("settings.proxy.errorLoad"));
        }
        try {
            const detected = await invoke<string | null>("detect_system_proxy");
            setSystemProxy(detected);
        } catch {
            // Non-critical
        }
        setIsLoading(false);
    };

    const handleSave = async () => {
        if (isLoading() || isSaving()) return;
        setError(null);
        setSaved(false);
        setIsSaving(true);
        try {
            await invoke("set_proxy_config", { mode: mode(), url: customUrl() });
            setSaved(true);
            window.clearTimeout(savedTimeout);
            savedTimeout = window.setTimeout(() => setSaved(false), 2000);
        } catch (err) {
            console.error("Failed to save proxy config:", getErrorMessage(err));
            setError(t("settings.proxy.errorSave"));
        } finally {
            setIsSaving(false);
        }
    };

    const handleTest = async () => {
        if (isTesting()) return;
        setTestResult(null);
        setTestError(null);
        setIsTesting(true);
        try {
            const result = await invoke<string>("test_proxy_connection", {
                mode: mode(),
                url: customUrl(),
            });
            setTestResult(result);
            window.clearTimeout(testTimeout);
            testTimeout = window.setTimeout(() => setTestResult(null), 5000);
        } catch (err) {
            setTestError(getErrorMessage(err));
            window.clearTimeout(testTimeout);
            testTimeout = window.setTimeout(() => setTestError(null), 5000);
        } finally {
            setIsTesting(false);
        }
    };

    const handleModeChange = (newMode: string) => {
        setMode(newMode);
        setError(null);
        setSaved(false);
    };

    const refreshSystemProxy = async () => {
        try {
            const detected = await invoke<string | null>("detect_system_proxy");
            setSystemProxy(detected);
        } catch {
            setSystemProxy(null);
        }
    };

    onMount(() => {
        fetchConfig();
    });

    onCleanup(() => {
        window.clearTimeout(savedTimeout);
        window.clearTimeout(testTimeout);
    });

    return (
        <Card
            title={t("settings.proxy.title")}
            icon={Globe}
            description={t("settings.proxy.description")}
        >
            {/* Mode selector */}
            <div class="flex flex-col gap-2 max-w-lg">
                <label for={modeId} class="text-sm font-medium">
                    {t("settings.proxy.modeLabel")}
                </label>
                <select
                    id={modeId}
                    class="select select-bordered select-sm w-full bg-base-100"
                    value={mode()}
                    onChange={(e) => handleModeChange(e.currentTarget.value)}
                    disabled={isLoading()}
                >
                    <option value="system">{t("settings.proxy.modeSystem")}</option>
                    <option value="custom">{t("settings.proxy.modeCustom")}</option>
                    <option value="none">{t("settings.proxy.modeNone")}</option>
                </select>
            </div>

            {/* System-detected proxy info */}
            <Show when={mode() === "system"}>
                <div class="mt-3 flex flex-col gap-1 text-sm">
                    <span class="text-base-content/60">{t("settings.proxy.detectedFromWindows")}</span>
                    <div class="flex items-center gap-2">
                        <Show
                            when={systemProxy()}
                            fallback={
                                <span class="text-base-content/60 italic">
                                    {isLoading() ? t("common.loading") : t("settings.proxy.noSystemProxy")}
                                </span>
                            }
                        >
                            <code class="badge badge-ghost font-mono text-xs">{systemProxy()}</code>
                        </Show>
                        <button
                            type="button"
                            class="btn btn-ghost btn-xs"
                            onClick={refreshSystemProxy}
                            disabled={isLoading()}
                        >
                            {t("common.refresh")}
                        </button>
                    </div>
                </div>
            </Show>

            {/* Custom proxy input */}
            <Show when={mode() === "custom"}>
                <div class="mt-3 flex flex-col gap-2 max-w-lg">
                    <label for={inputId} class="text-sm font-medium">
                        {t("settings.proxy.urlLabel")}
                    </label>
                    <input
                        id={inputId}
                        type="text"
                        placeholder={isLoading() ? t("common.loading") : t("settings.proxy.urlPlaceholder")}
                        class="input input-bordered input-sm w-full bg-base-100 font-mono text-sm focus:outline-none focus:border-base-content/20"
                        value={customUrl()}
                        onInput={(e) => {
                            setCustomUrl(e.currentTarget.value);
                            setError(null);
                        }}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault();
                                handleSave();
                            }
                        }}
                        disabled={isLoading()}
                        spellcheck={false}
                        autocomplete="off"
                        aria-describedby={statusId}
                    />
                    <p class="text-xs text-base-content/50">{t("settings.proxy.urlHint")}</p>
                </div>
            </Show>

            {/* Actions */}
            <div class="mt-4 flex flex-wrap items-center gap-2">
                <button
                    type="button"
                    class="btn btn-primary btn-sm"
                    onClick={handleSave}
                    disabled={isLoading() || isSaving() || (mode() === "custom" && !customUrl().trim())}
                >
                    {isSaving() ? t("common.loading") : saved() ? t("common.saved") : t("common.save")}
                </button>
                <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    onClick={handleTest}
                    disabled={isLoading() || isTesting()}
                >
                    {isTesting() ? t("settings.proxy.testing") : t("settings.proxy.testConnection")}
                </button>
            </div>

            <p id={statusId} class="text-xs mt-1 min-h-4" aria-live="polite">
                {error() ? <span class="text-error">{error()}</span> : saved() ? <span class="text-success">{t("common.saved")}</span> : null}
            </p>
            <p id={testStatusId} class="text-xs min-h-4" aria-live="polite">
                {testError() ? (
                    <span class="text-error">{t("settings.proxy.testFailed", { error: testError() ?? "" })}</span>
                ) : testResult() ? (
                    <span class="text-success">{t("settings.proxy.testSuccess", { result: testResult() ?? "" })}</span>
                ) : null}
            </p>
        </Card>
    );
}
