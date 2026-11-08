// 「检查状态」的全局状态。
//
// 检查会先更新 Bucket、刷新已安装列表，再调用 Scoop 状态检查，耗时可能较长。
// 把状态与检查函数放在模块级 store 中，切换页面（组件卸载）时检查仍会继续，
// 返回页面后 loading / 结果不会丢失；同时避免重复点击造成并发检查。
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import installedPackagesStore from "./installedPackagesStore";
import { getErrorMessage } from "../utils/errors";

export interface AppStatusInfo {
  name: string;
  installed_version: string;
  latest_version?: string;
  info: string[];
  is_outdated: boolean;
  is_failed: boolean;
  is_held: boolean;
  is_deprecated: boolean;
  is_removed: boolean;
}

export interface ScoopStatus {
  scoop_needs_update: boolean;
  bucket_needs_update: boolean;
  network_failure: boolean;
  apps_with_issues: AppStatusInfo[];
  is_everything_ok: boolean;
}

const [scoopStatus, setScoopStatus] = createSignal<ScoopStatus | null>(null);
const [statusLoading, setStatusLoading] = createSignal(false);
const [statusError, setStatusError] = createSignal<string | null>(null);

// 复用进行中的检查，避免用户重复点击或切换页面后再点造成并发。
let inflight: Promise<ScoopStatus | null> | null = null;

async function checkScoopStatus(): Promise<ScoopStatus | null> {
  if (inflight) return inflight;

  setStatusLoading(true);
  setStatusError(null);

  inflight = (async () => {
    try {
      // 先更新 Bucket，保证状态检查基于最新清单，也是发现新版本的前提。
      await invoke("update_all_buckets");
      // 刷新已安装列表（会同步刷新可用版本信息）。
      await installedPackagesStore.refetch();
      const status = await invoke<ScoopStatus>("check_scoop_status");
      setScoopStatus(status);
      return status;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      console.error("Failed to check scoop status:", errorMsg);
      setStatusError(errorMsg);
      return null;
    } finally {
      setStatusLoading(false);
      inflight = null;
    }
  })();

  return inflight;
}

const scoopStatusStore = {
  scoopStatus,
  statusLoading,
  statusError,
  checkScoopStatus,
};

export default scoopStatusStore;
