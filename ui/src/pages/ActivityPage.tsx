import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../store";
import { commands, SyncProgressEvent, SyncCompleteEvent, DiscoveredPeer } from "../lib/tauri";

interface SyncRun {
  runId: string;
  profileId: string;
  profileName: string;
  direction: string;
  status: "running" | "complete" | "error" | "cancelled";
  currentFile?: string;
  filesCompleted: number;
  filesTotal: number;
  bytesTransferred: number;
  bytesTotal: number;
  errorMessage?: string;
  startedAt: Date;
  completedAt?: Date;
}

export function ActivityPage() {
  const { profiles, fetchProfiles, peers, fetchPeers } = useStore();
  const [runs, setRuns] = useState<SyncRun[]>([]);
  const [selectedProfile, setSelectedProfile] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [discoveredPeers, setDiscoveredPeers] = useState<DiscoveredPeer[]>([]);

  useEffect(() => {
    fetchProfiles();
    fetchPeers();

    // Fetch discovered peers periodically to get current addresses
    const fetchDiscovered = async () => {
      try {
        const discovered = await commands.listDiscoveredPeers();
        setDiscoveredPeers(discovered);
      } catch (error) {
        console.error("Failed to fetch discovered peers:", error);
      }
    };

    fetchDiscovered();
    const interval = setInterval(fetchDiscovered, 5000); // Refresh every 5 seconds

    const unlistenProgress = listen<SyncProgressEvent>("sync:progress", (event) => {
      setRuns((prev) =>
        prev.map((run) =>
          run.runId === event.payload.run_id && run.status === "running"
            ? {
                ...run,
                currentFile: event.payload.current_file || undefined,
                filesCompleted: event.payload.files_completed,
                filesTotal: event.payload.files_total,
                bytesTransferred: event.payload.bytes_transferred,
                bytesTotal: event.payload.bytes_total,
              }
            : run
        )
      );
    });

    const unlistenComplete = listen<SyncCompleteEvent>("sync:complete", (event) => {
      setRuns((prev) =>
        prev.map((run) =>
          run.runId === event.payload.run_id && run.status === "running"
            ? {
                ...run,
                status: "complete",
                filesCompleted: event.payload.files_transferred,
                bytesTransferred: event.payload.bytes_transferred,
                completedAt: new Date(),
              }
            : run
        )
      );
    });

    const unlistenError = listen<{ run_id: string; profile_id: string; error: string }>(
      "sync:error",
      (event) => {
        setRuns((prev) =>
          prev.map((run) =>
            run.runId === event.payload.run_id && run.status === "running"
              ? {
                  ...run,
                  status: "error",
                  errorMessage: event.payload.error,
                  completedAt: new Date(),
                }
              : run
          )
        );
      }
    );

    const unlistenCancelled = listen<{ run_id: string; profile_id: string }>(
      "sync:cancelled",
      (event) => {
        setRuns((prev) =>
          prev.map((run) =>
            run.runId === event.payload.run_id && run.status === "running"
              ? {
                  ...run,
                  status: "cancelled",
                  completedAt: new Date(),
                }
              : run
          )
        );
      }
    );

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenCancelled.then((fn) => fn());
      clearInterval(interval);
    };
  }, [fetchProfiles, fetchPeers]);

  const handleCancel = async (runId: string) => {
    try {
      await commands.cancelSync(runId);
    } catch (error) {
      console.error("Failed to cancel sync:", error);
    }
  };

  const handleSync = async (direction: "push" | "pull" | "bidi") => {
    if (!selectedProfile) {
      alert("Please select a profile");
      return;
    }

    const profile = profiles.find((p) => p.id === selectedProfile);
    if (!profile || !profile.peer_name) {
      alert("Profile must have a paired peer");
      return;
    }

    // Find the paired peer
    const peer = peers.find((pr) => pr.name === profile.peer_name);
    if (!peer) {
      alert(`Peer "${profile.peer_name}" not found. Please pair with this peer first.`);
      return;
    }

    // Find the discovered peer to get current address
    const discovered = discoveredPeers.find((dp) => dp.id === peer.id);
    if (!discovered || discovered.addresses.length === 0) {
      alert(`Peer "${profile.peer_name}" is not currently discoverable. Please ensure the peer is running on the network.`);
      return;
    }

    // Use the first available address
    const peerAddress = discovered.addresses[0];

    setSyncing(true);
    try {
      const result = await commands.startSync(selectedProfile, peerAddress, direction);

      setRuns((prev) => [
        {
          runId: result.run_id,
          profileId: result.profile_id,
          profileName: profile?.name || "Unknown",
          direction: result.direction,
          status: "running",
          filesCompleted: 0,
          filesTotal: 0,
          bytesTransferred: 0,
          bytesTotal: 0,
          startedAt: new Date(),
        },
        ...prev,
      ]);
    } catch (error) {
      alert(`Sync failed: ${error}`);
    } finally {
      setSyncing(false);
    }
  };

  // Check if selected profile has a discoverable peer
  const selectedProfileData = selectedProfile ? profiles.find((p) => p.id === selectedProfile) : null;
  const selectedPeer = selectedProfileData?.peer_name
    ? peers.find((pr) => pr.name === selectedProfileData.peer_name)
    : null;
  const isPeerDiscoverable = selectedPeer
    ? discoveredPeers.some((dp) => dp.id === selectedPeer.id && dp.addresses.length > 0)
    : false;
  const canSync = selectedProfile && selectedProfileData?.peer_name && isPeerDiscoverable;

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold text-white mb-6">Sync Activity</h2>

      {/* Sync Controls */}
      <div className="glass-card p-4 mb-6">
        <h3 className="text-lg font-semibold text-white mb-3">Start Sync</h3>
        <div className="flex gap-3 items-end flex-wrap">
          <div className="flex-1 min-w-[200px]">
            <label className="block text-sm font-medium text-gray-400 mb-1">
              Select Profile
            </label>
            <select
              value={selectedProfile}
              onChange={(e) => setSelectedProfile(e.target.value)}
              disabled={syncing}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50"
            >
              <option value="" className="bg-gray-900">-- Choose a profile --</option>
              {profiles.map((p) => (
                <option key={p.id} value={p.id} className="bg-gray-900">
                  {p.name} ({p.mode})
                </option>
              ))}
            </select>
            {selectedProfile && selectedProfileData && !selectedProfileData.peer_name && (
              <p className="text-xs text-yellow-400 mt-1">⚠ Profile has no paired peer</p>
            )}
            {selectedProfile && selectedProfileData?.peer_name && !isPeerDiscoverable && (
              <p className="text-xs text-red-400 mt-1">⚠ Peer not discoverable on network</p>
            )}
            {selectedProfile && selectedProfileData?.peer_name && isPeerDiscoverable && (
              <p className="text-xs text-green-400 mt-1">✓ Peer online and ready</p>
            )}
          </div>
          <button
            onClick={() => handleSync("push")}
            disabled={syncing || !canSync}
            className="px-4 py-2 bg-blue-600/80 text-white rounded-lg hover:bg-blue-500 disabled:opacity-40 transition-colors"
          >
            Push
          </button>
          <button
            onClick={() => handleSync("pull")}
            disabled={syncing || !canSync}
            className="px-4 py-2 bg-green-600/80 text-white rounded-lg hover:bg-green-500 disabled:opacity-40 transition-colors"
          >
            Pull
          </button>
          <button
            onClick={() => handleSync("bidi")}
            disabled={syncing || !canSync}
            className="px-4 py-2 bg-purple-600/80 text-white rounded-lg hover:bg-purple-500 disabled:opacity-40 transition-colors"
          >
            Bidi
          </button>
        </div>
      </div>

      {/* Sync History */}
      <div>
        <h3 className="text-lg font-semibold text-white mb-3">Recent Syncs</h3>
        {runs.length === 0 ? (
          <div className="text-center py-8 text-gray-500">No syncs yet</div>
        ) : (
          <div className="space-y-3">
            {runs.map((run) => (
              <div
                key={run.runId}
                className={`glass-card p-4 ${
                  run.status === "error"
                    ? "border-red-500/30"
                    : run.status === "complete"
                    ? "border-green-500/20"
                    : "border-blue-500/20"
                }`}
              >
                <div className="flex justify-between items-start mb-2">
                  <div>
                    <h4 className="font-semibold text-white">{run.profileName}</h4>
                    <p className="text-sm text-gray-400">
                      {run.direction.toUpperCase()} · {run.startedAt.toLocaleTimeString()}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`px-2 py-1 text-xs font-medium rounded ${
                        run.status === "running"
                          ? "bg-blue-500/20 text-blue-300"
                          : run.status === "complete"
                          ? "bg-green-500/20 text-green-300"
                          : run.status === "cancelled"
                          ? "bg-gray-500/20 text-gray-300"
                          : "bg-red-500/20 text-red-300"
                      }`}
                    >
                      {run.status}
                    </span>
                    {run.status === "running" && (
                      <button
                        onClick={() => handleCancel(run.runId)}
                        className="px-2 py-1 text-xs text-red-400 hover:text-red-300 hover:bg-red-400/10 rounded transition-colors"
                        title="Cancel sync"
                      >
                        Cancel
                      </button>
                    )}
                  </div>
                </div>
                {run.status === "running" && (
                  <div className="mt-3 space-y-2">
                    {run.currentFile && (
                      <p className="text-xs text-blue-300 truncate">
                        📄 {run.currentFile}
                      </p>
                    )}
                    <div className="w-full bg-white/5 rounded-full h-2">
                      <div
                        className="bg-gradient-to-r from-blue-500 to-purple-500 h-2 rounded-full transition-all"
                        style={{
                          width: run.filesTotal > 0
                            ? `${(run.filesCompleted / run.filesTotal) * 100}%`
                            : "0%",
                        }}
                      />
                    </div>
                    <div className="flex justify-between text-xs text-gray-400">
                      <span>
                        {run.filesCompleted} / {run.filesTotal} files
                      </span>
                      <span>
                        {(run.bytesTransferred / 1024 / 1024).toFixed(1)} MB
                        {run.bytesTotal > 0 &&
                          ` / ${(run.bytesTotal / 1024 / 1024).toFixed(1)} MB`}
                      </span>
                    </div>
                  </div>
                )}
                {run.status === "complete" && (
                  <div className="mt-2 text-sm text-gray-400">
                    ✓ {run.filesCompleted} files · {(run.bytesTransferred / 1024 / 1024).toFixed(1)} MB
                  </div>
                )}
                {run.status === "cancelled" && (
                  <div className="mt-2 text-sm text-gray-400">
                    Cancelled after {run.filesCompleted} files · {(run.bytesTransferred / 1024 / 1024).toFixed(1)} MB
                  </div>
                )}
                {run.status === "error" && run.errorMessage && (
                  <div className="mt-2 text-sm text-red-400">
                    {run.errorMessage}
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
