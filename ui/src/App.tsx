import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "./store";
import { TabBar } from "./components/TabBar";
import { ProfilesPage } from "./pages/ProfilesPage";
import { PeersPage } from "./pages/PeersPage";
import { ActivityPage } from "./pages/ActivityPage";
import { DeletionPrompt } from "./components/DeletionPrompt";
import { ConflictNotice } from "./components/ConflictNotice";

function App() {
  const { activeTab, pendingDeletions, fetchPendingDeletions } = useStore();
  const [conflictNotice, setConflictNotice] = useState<string | null>(null);

  useEffect(() => {
    fetchPendingDeletions();

    const unlistenConflict = listen<{ profile_name: string }>(
      "profile:conflict-resolved",
      (event) => {
        setConflictNotice(event.payload.profile_name);
        setTimeout(() => setConflictNotice(null), 5000);
      }
    );

    return () => {
      unlistenConflict.then((fn) => fn());
    };
  }, [fetchPendingDeletions]);

  return (
    <div className="min-h-screen">
      <header className="glass border-b border-white/10">
        <div className="px-6 py-4 flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center">
            <svg className="w-4 h-4 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
            </svg>
          </div>
          <h1 className="text-xl font-bold text-white">FileSync</h1>
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
