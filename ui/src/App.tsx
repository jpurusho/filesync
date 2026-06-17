import { useEffect } from "react";
import { useStore } from "./store";
import { TabBar } from "./components/TabBar";
import { ProfilesPage } from "./pages/ProfilesPage";
import { PeersPage } from "./pages/PeersPage";
import { ActivityPage } from "./pages/ActivityPage";
import { DeletionPrompt } from "./components/DeletionPrompt";

function App() {
  const { activeTab, pendingDeletions, fetchPendingDeletions } = useStore();

  useEffect(() => {
    fetchPendingDeletions();
  }, [fetchPendingDeletions]);

  return (
    <div className="min-h-screen bg-gray-50">
      <header className="bg-white shadow">
        <div className="px-6 py-4">
          <h1 className="text-2xl font-bold text-gray-900">FileSync</h1>
        </div>
      </header>
      <TabBar />
      <main>
        {activeTab === "profiles" && <ProfilesPage />}
        {activeTab === "peers" && <PeersPage />}
        {activeTab === "activity" && <ActivityPage />}
      </main>

      {/* Deletion prompts - show one at a time */}
      {pendingDeletions.length > 0 && (
        <DeletionPrompt
          profile={pendingDeletions[0]}
          onResolved={fetchPendingDeletions}
        />
      )}
    </div>
  );
}

export default App;
