import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { useStore } from "./store";
import { TabBar } from "./components/TabBar";
import { ProfilesPage } from "./pages/ProfilesPage";
import { PeersPage } from "./pages/PeersPage";
import { ActivityPage } from "./pages/ActivityPage";
import { DeletionPrompt } from "./components/DeletionPrompt";
import { ConflictNotice } from "./components/ConflictNotice";
import { UpdateInstructions } from "./components/UpdateInstructions";

function App() {
  const { activeTab, pendingDeletions, fetchPendingDeletions } = useStore();
  const [conflictNotice, setConflictNotice] = useState<string | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [updateAvailable, setUpdateAvailable] = useState<Update | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [showUpdateInstructions, setShowUpdateInstructions] = useState(false);

  useEffect(() => {
    fetchPendingDeletions();

    const unlistenConflict = listen<{ profile_name: string }>(
      "profile:conflict-resolved",
      (event) => {
        setConflictNotice(event.payload.profile_name);
        setTimeout(() => setConflictNotice(null), 5000);
      }
    );

    // Get app version
    getVersion().then(setAppVersion).catch(() => setAppVersion("0.0.0"));

    // Check for updates on launch (delayed by 3 seconds)
    const timer = setTimeout(() => {
      check()
        .then((update) => {
          if (update) {
            setUpdateAvailable(update);
          }
        })
        .catch(() => {
          // Silent fail on update check
        });
    }, 3000);

    return () => {
      unlistenConflict.then((fn) => fn());
      clearTimeout(timer);
    };
  }, [fetchPendingDeletions]);

  const handleInstallUpdate = async () => {
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

      // Show instructions modal for unsigned builds
      setShowUpdateInstructions(true);
      setUpdateProgress(null);
    } catch (error) {
      alert(`Failed to install update: ${error}`);
      setUpdateProgress(null);
    }
  };

  return (
    <div className="min-h-screen">
      <header className="glass border-b border-white/10">
        <div className="px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center">
              <svg className="w-4 h-4 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
              </svg>
            </div>
            <h1 className="text-xl font-bold text-white">FileSync</h1>
          </div>
          <div className="flex items-center gap-2">
            {/* Version / Update button */}
            {updateAvailable ? (
              <button
                onClick={handleInstallUpdate}
                disabled={updateProgress !== null}
                className="px-3 py-1.5 text-sm bg-amber-500 text-white rounded-lg hover:bg-amber-400 disabled:opacity-50 transition-colors flex items-center gap-1.5 animate-pulse"
                title={`Update to v${updateAvailable.version}`}
              >
                {updateProgress !== null ? (
                  <span className="tabular-nums font-medium">{updateProgress}%</span>
                ) : (
                  <>
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                    </svg>
                    <span className="font-medium">v{updateAvailable.version}</span>
                  </>
                )}
              </button>
            ) : appVersion ? (
              <span className="px-3 py-1.5 text-sm text-gray-400 tabular-nums">
                v{appVersion}
              </span>
            ) : null}
          </div>
        </div>
      </header>
      <TabBar />
      <main>
        {activeTab === "profiles" && <ProfilesPage />}
        {activeTab === "peers" && <PeersPage />}
        {activeTab === "activity" && <ActivityPage />}
      </main>

      {pendingDeletions.length > 0 && (
        <DeletionPrompt
          profile={pendingDeletions[0]}
          onResolved={fetchPendingDeletions}
        />
      )}

      {conflictNotice && (
        <ConflictNotice
          profileName={conflictNotice}
          onClose={() => setConflictNotice(null)}
        />
      )}

      {showUpdateInstructions && (
        <UpdateInstructions
          onClose={() => setShowUpdateInstructions(false)}
        />
      )}
    </div>
  );
}

export default App;
