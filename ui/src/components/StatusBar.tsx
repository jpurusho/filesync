import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { useStore } from "../store";

export function StatusBar() {
  const { profiles, peers, activeTab } = useStore();
  const [appVersion, setAppVersion] = useState("");
  const [updateAvailable, setUpdateAvailable] = useState<Update | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => setAppVersion("0.0.0"));

    // Check for updates (delayed)
    const timer = setTimeout(() => {
      check()
        .then((update) => {
          if (update) setUpdateAvailable(update);
        })
        .catch(() => {});
    }, 3000);

    return () => clearTimeout(timer);
  }, []);

  const handleUpdate = async () => {
    if (!updateAvailable || updateProgress !== null) return;

    setUpdateProgress(0);
    try {
      let total = 0;
      let downloaded = 0;

      await updateAvailable.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) {
            setUpdateProgress(Math.round((downloaded / total) * 100));
          }
        }
      });

      // Note: For unsigned builds, relaunch won't work automatically
      // User will need to manually restart the app
      setUpdateProgress(null);
    } catch (error) {
      console.error("Update failed:", error);
      setUpdateProgress(null);
    }
  };

  const profileCount = profiles.filter((p) => !p.pending_deletion).length;
  const peerCount = peers.length;
  const onlinePeerCount = peers.filter((p) => p.is_online).length;
  const pendingDeletionCount = profiles.filter((p) => p.pending_deletion).length;

  return (
    <div className="h-8 bg-gray-900/95 backdrop-blur-xl border-t border-white/10 flex items-center px-4 gap-3 text-xs text-gray-400 shrink-0">
      {/* Current tab indicator */}
      <div className="flex items-center gap-1.5">
        <span className="capitalize font-medium text-gray-300">{activeTab}</span>
      </div>

      {/* Separator */}
      <div className="w-px h-4 bg-white/10" />

      {/* Stats */}
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-1">
          <svg className="w-3 h-3 text-blue-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <span className="tabular-nums">{profileCount} {profileCount === 1 ? "profile" : "profiles"}</span>
        </span>

        {pendingDeletionCount > 0 && (
          <span className="flex items-center gap-1 text-yellow-400">
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
            <span className="tabular-nums">{pendingDeletionCount} pending</span>
          </span>
        )}

        <span className="flex items-center gap-1">
          <svg className="w-3 h-3 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
          <span className="tabular-nums">
            {onlinePeerCount > 0 ? (
              <span className="text-green-400">{onlinePeerCount}</span>
            ) : (
              <span>{peerCount}</span>
            )}
            {" "}{peerCount === 1 ? "peer" : "peers"}
            {onlinePeerCount > 0 && peerCount > onlinePeerCount && (
              <span className="text-gray-500"> ({peerCount} total)</span>
            )}
          </span>
        </span>
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Version / Update */}
      {updateAvailable ? (
        <button
          onClick={handleUpdate}
          disabled={updateProgress !== null}
          className="flex items-center gap-1.5 px-2 py-0.5 rounded text-amber-400 hover:bg-amber-400/10 transition-colors animate-pulse"
          title={`Update to v${updateAvailable.version}`}
        >
          {updateProgress !== null ? (
            <span className="tabular-nums font-medium">{updateProgress}%</span>
          ) : (
            <>
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
              <span className="font-medium">v{updateAvailable.version}</span>
            </>
          )}
        </button>
      ) : appVersion ? (
        <span className="tabular-nums text-gray-500">v{appVersion}</span>
      ) : null}
    </div>
  );
}
