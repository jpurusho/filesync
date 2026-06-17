import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../store";
import { commands, SyncProgressEvent, SyncCompleteEvent } from "../lib/tauri";

interface SyncRun {
  runId: string;
  profileId: string;
  profileName: string;
  direction: string;
  status: "running" | "complete" | "error";
  progress: number;
  filesTransferred?: number;
  bytesTransferred?: number;
  startedAt: Date;
  completedAt?: Date;
}

export function ActivityPage() {
  const { profiles, fetchProfiles } = useStore();
  const [runs, setRuns] = useState<SyncRun[]>([]);
  const [selectedProfile, setSelectedProfile] = useState("");
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    fetchProfiles();

    const unlistenProgress = listen<SyncProgressEvent>("sync-progress", (event) => {
      setRuns((prev) =>
        prev.map((run) =>
          run.profileId === event.payload.profile_id && run.status === "running"
            ? { ...run, progress: event.payload.progress, status: "running" }
            : run
        )
      );
    });

    const unlistenComplete = listen<SyncCompleteEvent>("sync-complete", (event) => {
      setRuns((prev) =>
        prev.map((run) =>
          run.profileId === event.payload.profile_id && run.status === "running"
            ? {
                ...run,
                status: "complete",
                progress: 1.0,
                filesTransferred: event.payload.files_transferred,
                bytesTransferred: event.payload.bytes_transferred,
                completedAt: new Date(),
              }
            : run
        )
      );
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
    };
  }, [fetchProfiles]);

  const handleSync = async (direction: "push" | "pull" | "bidi") => {
    if (!selectedProfile) {
      alert("Please select a profile");
      return;
    }

    setSyncing(true);
    try {
      const result = await commands.startSync(selectedProfile, direction);
      const profile = profiles.find((p) => p.id === selectedProfile);

      setRuns((prev) => [
        {
          runId: result.run_id,
          profileId: result.profile_id,
          profileName: profile?.name || "Unknown",
          direction: result.direction,
          status: "running",
          progress: 0,
          startedAt: new Date(),
        },
        ...prev,
      ]);
    } catch (error) {
      console.error("Failed to start sync:", error);
      alert(`Sync failed: ${error}`);
    } finally {
      setSyncing(false);
    }
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold text-gray-900 mb-6">Sync Activity</h2>

      {/* Sync Controls */}
      <div className="bg-white border border-gray-200 rounded-lg p-4 mb-6">
        <h3 className="text-lg font-semibold mb-3">Start Sync</h3>
        <div className="flex gap-3 items-end">
          <div className="flex-1">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Select Profile
            </label>
            <select
              value={selectedProfile}
              onChange={(e) => setSelectedProfile(e.target.value)}
              disabled={syncing}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="">-- Choose a profile --</option>
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.mode})
                </option>
              ))}
            </select>
          </div>
          <button
            onClick={() => handleSync("push")}
            disabled={syncing || !selectedProfile}
            className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
          >
            Push
          </button>
          <button
            onClick={() => handleSync("pull")}
            disabled={syncing || !selectedProfile}
            className="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50"
          >
            Pull
          </button>
          <button
            onClick={() => handleSync("bidi")}
            disabled={syncing || !selectedProfile}
            className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50"
          >
            Bidi
          </button>
        </div>
      </div>

      {/* Sync History */}
      <div>
        <h3 className="text-lg font-semibold mb-3">Recent Syncs</h3>
        {runs.length === 0 ? (
          <div className="text-center py-8 text-gray-500">No syncs yet</div>
        ) : (
          <div className="space-y-3">
            {runs.map((run) => (
              <div
                key={run.runId}
                className="bg-white border border-gray-200 rounded-lg p-4"
              >
                <div className="flex justify-between items-start mb-2">
                  <div>
                    <h4 className="font-semibold">{run.profileName}</h4>
                    <p className="text-sm text-gray-600">
                      {run.direction.toUpperCase()} · {run.startedAt.toLocaleTimeString()}
                    </p>
                  </div>
                  <span
                    className={`px-2 py-1 text-xs font-medium rounded ${
                      run.status === "running"
                        ? "bg-blue-100 text-blue-700"
                        : run.status === "complete"
                        ? "bg-green-100 text-green-700"
                        : "bg-red-100 text-red-700"
                    }`}
                  >
                    {run.status}
                  </span>
                </div>
                {run.status === "running" && (
                  <div className="mt-2">
                    <div className="w-full bg-gray-200 rounded-full h-2">
                      <div
                        className="bg-blue-600 h-2 rounded-full transition-all"
                        style={{ width: `${run.progress * 100}%` }}
                      />
                    </div>
                    <p className="text-xs text-gray-500 mt-1">
                      {Math.round(run.progress * 100)}%
                    </p>
                  </div>
                )}
                {run.status === "complete" && (
                  <div className="mt-2 text-sm text-gray-600">
                    {run.filesTransferred} files · {(run.bytesTransferred! / 1024).toFixed(1)} KB
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
