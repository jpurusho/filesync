import { useStore } from "./store";
import { TabBar } from "./components/TabBar";
import { ProfilesPage } from "./pages/ProfilesPage";
import { PeersPage } from "./pages/PeersPage";
import { ActivityPage } from "./pages/ActivityPage";

function App() {
  const { activeTab } = useStore();

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
    </div>
  );
}

export default App;
