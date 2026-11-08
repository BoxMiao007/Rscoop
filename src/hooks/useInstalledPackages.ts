import { createSignal, onMount, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { createStoredSignal } from "./createStoredSignal";
import heldStore from "../stores/held";
import installedPackagesStore from "../stores/installedPackagesStore";
import scoopStatusStore from "../stores/scoopStatusStore";
import { usePackageInfo } from "./usePackageInfo";
import operationsStore from "../stores/operations";
import type { ScoopPackage } from "../types/scoop";
import { getErrorMessage } from "../utils/errors";

export type { AppStatusInfo, ScoopStatus } from "../stores/scoopStatusStore";

type SortKey = 'name' | 'version' | 'source' | 'updated';

export function useInstalledPackages() {
  const { packages, loading, error, uniqueBuckets, isCheckingForUpdates, isPackageVersioned, fetch, refetch } = installedPackagesStore;
  const { scoopStatus, statusLoading, statusError, checkScoopStatus } = scoopStatusStore;
  const [operatingOn, setOperatingOn] = createSignal<string | null>(null);

  const packageInfo = usePackageInfo();

  // State for auto-showing versions in modal
  const [autoShowVersions, setAutoShowVersions] = createSignal(false);

  const [viewMode, setViewMode] = createStoredSignal<'grid' | 'list'>('installedViewMode', 'grid');
  const [sortKey, setSortKey] = createStoredSignal<SortKey>('installedSortKey', 'name');
  const [sortDirection, setSortDirection] = createStoredSignal<'asc' | 'desc'>('installedSortDirection', 'asc');
  const [selectedBucket, setSelectedBucket] = createStoredSignal<string>('installedSelectedBucket', 'all');

  onMount(fetch);

  const checkForUpdates = () => {
    installedPackagesStore.checkForUpdates();
  };

  const fetchInstalledPackages = () => {
    refetch();
  }

  const handleFetchPackageInfoForVersions = (pkg: ScoopPackage) => {
    setAutoShowVersions(true);
    packageInfo.fetchPackageInfo(pkg);
  }

  const handleFetchPackageInfo = (pkg: ScoopPackage) => {
    setAutoShowVersions(false);
    packageInfo.fetchPackageInfo(pkg);
  }

  const handleCloseInfoModalWithVersions = () => {
    setAutoShowVersions(false);
    packageInfo.closeModal();
  }

  const handleSort = (key: SortKey) => {
    if (sortKey() === key) {
      setSortDirection(prev => (prev === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDirection('asc');
    }
  };

  const handleHold = async (pkgName: string) => {
    setOperatingOn(pkgName);
    try {
      await invoke("hold_package", { packageName: pkgName });
    } catch (err) {
      console.error(`Failed to hold package ${pkgName}:`, getErrorMessage(err));
    } finally {
      await heldStore.refetch();
      installedPackagesStore.checkForUpdates();
      setOperatingOn(null);
    }
  };

  const handleUnhold = async (pkgName: string) => {
    setOperatingOn(pkgName);
    try {
      await invoke("unhold_package", { packageName: pkgName });
    } catch (err) {
      console.error(`Failed to unhold package ${pkgName}:`, getErrorMessage(err));
    } finally {
      await heldStore.refetch();
      installedPackagesStore.checkForUpdates();
      setOperatingOn(null);
    }
  };

  const handleSwitchVersion = async (pkgName: string, version: string) => {
    setOperatingOn(pkgName);
    try {
      await invoke("switch_package_version", {
        packageName: pkgName,
        targetVersion: version,
        global: false, // TODO: Add support for global packages
      });
    } catch (err) {
      console.error(`Failed to switch package ${pkgName} to version ${version}:`, getErrorMessage(err));
    } finally {
      // Refresh packages list to reflect any changes
      await refetch();
      setOperatingOn(null);
    }
  };

  const refreshAfterOperation = async () => {
    await refetch();
    const currentSelected = packageInfo.selectedPackage();
    if (currentSelected) {
      const updatedPackage = packages().find(p => p.name === currentSelected.name);
      if (updatedPackage) {
        packageInfo.updateSelectedPackage(updatedPackage);
      } else {
        packageInfo.updateSelectedPackage({ ...currentSelected, is_installed: false });
      }
    }
  };

  const handleUpdate = (pkg: ScoopPackage) => {
    operationsStore.queueUpdate(pkg, (wasSuccess) => {
      if (wasSuccess) refreshAfterOperation();
    });
  };

  const handleUpdateAll = () => {
    // 明确传入本次需要更新的包名，后端逐个更新并在单个失败时继续下一个。
    const packagesToUpdate = packages()
      .filter(p => !!p.available_version && !heldStore.isHeld(p.name) && !p.is_versioned_install)
      .map(p => p.name);
    operationsStore.queueUpdateAll(packagesToUpdate, (wasSuccess) => {
      if (wasSuccess) refreshAfterOperation();
    });
  };

  const handleUninstall = (pkg: ScoopPackage) => {
    operationsStore.queueUninstall(pkg, (wasSuccess) => {
      if (wasSuccess) refreshAfterOperation();
    });
  };

  const processedPackages = createMemo(() => {
    let pkgs = [...packages()];
    if (selectedBucket() !== 'all') {
      pkgs = pkgs.filter(p => p.source === selectedBucket());
    }
    const key = sortKey();
    const direction = sortDirection();
    const sortedPkgs = [...pkgs];
    sortedPkgs.sort((a, b) => {
      if (key === 'name') {
        const aHasUpdate = !!a.available_version && !heldStore.isHeld(a.name) && !a.is_versioned_install;
        const bHasUpdate = !!b.available_version && !heldStore.isHeld(b.name) && !b.is_versioned_install;
        if (aHasUpdate && !bHasUpdate) return -1;
        if (!aHasUpdate && bHasUpdate) return 1;
      }
      const valA = a[key].toLowerCase();
      const valB = b[key].toLowerCase();
      if (valA < valB) return direction === 'asc' ? -1 : 1;
      if (valA > valB) return direction === 'asc' ? 1 : -1;
      return 0;
    });
    return sortedPkgs;
  });

  const updatableCount = () => packages().filter(p => !!p.available_version && !heldStore.isHeld(p.name) && !p.is_versioned_install).length;

  return {
    loading,
    error,
    uniqueBuckets,
    isCheckingForUpdates,
    processedPackages,
    updatableCount,
    viewMode,
    setViewMode,
    sortKey,
    sortDirection,
    selectedBucket,
    setSelectedBucket,
    operatingOn,
    isPackageVersioned,
    autoShowVersions,

    // Status functionality
    scoopStatus,
    statusLoading,
    statusError,
    checkScoopStatus,

    // From usePackageInfo
    selectedPackage: packageInfo.selectedPackage,
    info: packageInfo.info,
    infoLoading: packageInfo.loading,
    infoError: packageInfo.error,
    handleFetchPackageInfo,
    handleFetchPackageInfoForVersions,
    handleCloseInfoModal: packageInfo.closeModal,
    handleCloseInfoModalWithVersions,

    // Operations
    handleUpdate,
    handleUpdateAll,
    handleUninstall,

    // Local methods
    handleSort,
    handleHold,
    handleUnhold,
    handleSwitchVersion,
    fetchInstalledPackages,
    checkForUpdates,
  };
} 
