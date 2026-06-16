import { useStore } from "../store";

export function TabBar() {
  const { activeTab, setActiveTab } = useStore();

  const tabs = [
    { id: "profiles" as const, label: "Profiles" },
    { id: "peers" as const, label: "Peers" },
    { id: "activity" as const, label: "Activity" },
  ];

  return (
    <div className="border-b border-gray-200">
      <nav className="flex space-x-8 px-6" aria-label="Tabs">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`
              py-4 px-1 border-b-2 font-medium text-sm
              ${
                activeTab === tab.id
                  ? "border-blue-500 text-blue-600"
                  : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300"
              }
            `}
          >
            {tab.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
