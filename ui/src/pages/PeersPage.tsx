import { useEffect, useState } from "react";
import { useStore } from "../store";
import { PeerView, commands } from "../lib/tauri";
import { PairForm } from "../components/PairForm";

export function PeersPage() {
  const { peers, loadingPeers, fetchPeers } = useStore();
  const [showPairForm, setShowPairForm] = useState(false);
  const [unpairingId, setUnpairingId] = useState<string | null>(null);

  useEffect(() => {
    fetchPeers();
  }, [fetchPeers]);

  const handleUnpair = async (peerId: string, peerName: string) => {
    if (!confirm(`Remove peer "${peerName}"?`)) return;

    setUnpairingId(peerId);
    try {
      await commands.unpairPeer(peerId);
      await fetchPeers();
    } catch (error) {
      console.error("Failed to unpair peer:", error);
      alert(`Failed to unpair: ${error}`);
    } finally {
      setUnpairingId(null);
    }
  };

  const handlePaired = async () => {
    setShowPairForm(false);
    await fetchPeers();
  };

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
        <button
          onClick={() => setShowPairForm(true)}
          className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
        >
          Pair New Peer
        </button>
      </div>

      {peers.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-gray-500 mb-4">No peers paired yet</p>
          <button
            onClick={() => setShowPairForm(true)}
            className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
          >
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
              <div className="flex items-start justify-between mb-3">
                <div className="flex-1">
                  <h3 className="text-lg font-semibold text-gray-900">{peer.name}</h3>
                  <div className="mt-2 space-y-1 text-sm text-gray-600">
                    <p className="font-mono text-xs truncate" title={peer.fingerprint}>
                      {peer.fingerprint.slice(0, 23)}...
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
              <button
                onClick={() => handleUnpair(peer.id, peer.name)}
                disabled={unpairingId === peer.id}
                className="w-full px-3 py-1 text-sm border border-red-300 text-red-600 rounded hover:bg-red-50 disabled:opacity-50"
              >
                {unpairingId === peer.id ? "Removing..." : "Remove"}
              </button>
            </div>
          ))}
        </div>
      )}

      {showPairForm && (
        <PairForm
          onPaired={handlePaired}
          onCancel={() => setShowPairForm(false)}
        />
      )}
    </div>
  );
}
