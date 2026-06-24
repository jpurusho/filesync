import { useStore } from "../store";

const tabs = [
  { id: "profiles" as const, label: "Profiles" },
  { id: "peers" as const, label: "Peers" },
  { id: "activity" as const, label: "Activity" },
];

export function TabBar() {
  const { activeTab, setActiveTab } = useStore();

  return (
    <div className="px-6 pt-4">
      <nav className="flex gap-1 glass-card p-1">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 px-4 py-2 text-sm font-medium rounded-lg transition-all duration-200 ${
              activeTab === tab.id
                ? "bg-white/15 text-white shadow-sm"
                : "text-gray-400 hover:text-white hover:bg-white/5"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
