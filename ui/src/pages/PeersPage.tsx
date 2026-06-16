import { useEffect } from "react";
import { useStore } from "../store";
import { PeerView } from "../lib/tauri";

export function PeersPage() {
  const { peers, loadingPeers, fetchPeers } = useStore();

  useEffect(() => {
    fetchPeers();
  }, [fetchPeers]);

  if (loadingPeers) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-gray-500">Loading peers...</p>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="flex justify-between items-center mb-6">
        <h2 className="text-2xl font-bold text-gray-900">Paired Peers</h2>
        <button className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700">
          Pair New Peer
        </button>
      </div>

      {peers.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-gray-500 mb-4">No peers paired yet</p>
          <button className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700">
            Pair your first peer
          </button>
        </div>
      ) : (
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {peers.map((peer: PeerView) => (
            <div
              key={peer.id}
              className="border border-gray-200 rounded-lg p-4 hover:shadow-md transition-shadow"
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <h3 className="text-lg font-semibold text-gray-900">{peer.name}</h3>
                  <div className="mt-2 space-y-1 text-sm text-gray-600">
                    <p className="font-mono text-xs truncate" title={peer.fingerprint}>
                      {peer.fingerprint.slice(0, 16)}...
                    </p>
                    <p className="text-xs text-gray-500">
                      Paired: {new Date(peer.paired_at).toLocaleString()}
                    </p>
                    {peer.last_seen && (
                      <p className="text-xs text-gray-500">
                        Last seen: {new Date(peer.last_seen).toLocaleString()}
                      </p>
                    )}
                  </div>
                </div>
                <div
                  className={`w-3 h-3 rounded-full ${
                    peer.is_online ? "bg-green-500" : "bg-gray-300"
                  }`}
                  title={peer.is_online ? "Online" : "Offline"}
                />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
