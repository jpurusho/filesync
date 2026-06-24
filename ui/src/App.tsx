import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "./store";
import { TabBar } from "./components/TabBar";
import { ProfilesPage } from "./pages/ProfilesPage";
import { PeersPage } from "./pages/PeersPage";
import { ActivityPage } from "./pages/ActivityPage";
import { DeletionPrompt } from "./components/DeletionPrompt";
import { ConflictNotice } from "./components/ConflictNotice";
import { checkForUpdates, installUpdate } from "./lib/tauri";

function App() {
  const { activeTab, pendingDeletions, fetchPendingDeletions } = useStore();
  const [conflictNotice, setConflictNotice] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    fetchPendingDeletions();

    const unlistenConflict = listen<{ profile_name: string }>(
      "profile:conflict-resolved",
      (event) => {
        setConflictNotice(event.payload.profile_name);
        setTimeout(() => setConflictNotice(null), 5000);
      }
    );

    // Check for updates on launch
    checkForUpdates()
      .then((result) => {
        if (result.shouldUpdate) {
          setUpdateAvailable(true);
        }
      })
      .catch(() => {
        // Silent fail on update check
      });

    return () => {
      unlistenConflict.then((fn) => fn());
    };
  }, [fetchPendingDeletions]);

  const handleCheckUpdates = async () => {
    setChecking(true);
    try {
      const result = await checkForUpdates();
      if (result.shouldUpdate) {
        setUpdateAvailable(true);
      } else {
        alert(`You're on the latest version (${result.currentVersion})`);
      }
    } catch (error) {
      alert(`Failed to check for updates: ${error}`);
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      await installUpdate();
      alert("Update installed! Please restart the app.");
    } catch (error) {
      alert(`Failed to install update: ${error}`);
    } finally {
      setInstalling(false);
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
            {updateAvailable && (
              <button
                onClick={handleInstallUpdate}
                disabled={installing}
                className="px-3 py-1.5 text-sm bg-green-600 text-white rounded-lg hover:bg-green-500 disabled:opacity-50 transition-colors flex items-center gap-1"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                {installing ? "Installing..." : "Update Available"}
              </button>
            )}
            <button
              onClick={handleCheckUpdates}
              disabled={checking}
              className="px-3 py-1.5 text-sm border border-white/10 text-gray-300 rounded-lg hover:bg-white/5 disabled:opacity-50 transition-colors"
              title="Check for updates"
            >
              {checking ? "Checking..." : "Check Updates"}
            </button>
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
    </div>
  );
}

export default App;
