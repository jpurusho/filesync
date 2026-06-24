import { useEffect, useState } from "react";
import { useStore } from "../store";
import { PeerView, NetworkInfo, DiscoveredPeer, commands } from "../lib/tauri";
import { PairForm } from "../components/PairForm";

export function PeersPage() {
  const { peers, loadingPeers, fetchPeers } = useStore();
  const [showPairForm, setShowPairForm] = useState(false);
  const [unpairingId, setUnpairingId] = useState<string | null>(null);
  const [networkInfo, setNetworkInfo] = useState<NetworkInfo | null>(null);
  const [discoveredPeers, setDiscoveredPeers] = useState<DiscoveredPeer[]>([]);
  const [pairAddress, setPairAddress] = useState("");

  useEffect(() => {
    fetchPeers();
    loadNetworkInfo();
    const interval = setInterval(() => {
      loadDiscoveredPeers();
      if (!networkInfo) loadNetworkInfo();
    }, 3000);
    loadDiscoveredPeers();
    return () => clearInterval(interval);
  }, [fetchPeers]);

  const loadNetworkInfo = async () => {
    try {
      const info = await commands.getNetworkInfo();
      setNetworkInfo(info);
    } catch (error) {
      // Network may not be initialized yet — retry on interval
    }
  };

  const loadDiscoveredPeers = async () => {
    try {
      const peers = await commands.listDiscoveredPeers();
      setDiscoveredPeers(peers);
    } catch (error) {
      // Silent retry
    }
  };

  const handleUnpair = async (peerId: string, peerName: string) => {
    if (!confirm(`Remove peer "${peerName}"?`)) return;

    setUnpairingId(peerId);
    try {
      await commands.unpairPeer(peerId);
      await fetchPeers();
    } catch (error) {
      alert(`Failed to unpair: ${error}`);
    } finally {
      setUnpairingId(null);
    }
  };

  const handlePaired = async () => {
    setShowPairForm(false);
    await fetchPeers();
  };

  const handlePairDiscovered = (addr: string) => {
    setPairAddress(addr);
    setShowPairForm(true);
  };

  if (loadingPeers) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-gray-400">Loading peers...</p>
      </div>
    );
  }

  return (
    <div className="p-6">
      {/* Network Info Banner */}
      {networkInfo && (
        <div className="mb-6 glass-card p-4 bg-gradient-to-r from-blue-500/10 to-indigo-500/10 border-blue-500/20">
          <div className="flex items-center gap-2 mb-2">
            <div className="w-2 h-2 rounded-full bg-green-400 glow-green animate-pulse" />
            <h3 className="text-sm font-semibold text-blue-200">This Device — Listening</h3>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-2 text-sm">
            <div>
              <span className="text-gray-400">Address:</span>{" "}
              <span className="font-mono font-semibold text-white">
                {networkInfo.listen_address}
              </span>
            </div>
            <div>
              <span className="text-gray-400">Host:</span>{" "}
              <span className="text-gray-200">{networkInfo.hostname}</span>
            </div>
            <div>
              <span className="text-gray-400">FP:</span>{" "}
              <span className="font-mono text-xs text-gray-300">{networkInfo.fingerprint}</span>
            </div>
          </div>
          <p className="text-xs text-gray-500 mt-2">
            Share this address with the other machine to pair.
          </p>
        </div>
      )}

      {/* Discovered Peers Section */}
      {discoveredPeers.length > 0 && (
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-white mb-3 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
            Discovered on Network ({discoveredPeers.length})
          </h3>
          <div className="grid gap-3 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
            {discoveredPeers.map((dp) => (
              <div
                key={dp.id}
                className="glass-card p-3 bg-gradient-to-br from-emerald-500/10 to-green-600/5 border-emerald-500/20"
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <h4 className="font-semibold text-emerald-200">{dp.name}</h4>
                    <p className="text-xs text-emerald-300/70 font-mono mt-1">
                      {dp.addresses.join(", ")}
                    </p>
                    <p className="text-xs text-gray-500 mt-1">
                      FP: {dp.fingerprint_short}
                    </p>
                  </div>
                  <div className="w-2.5 h-2.5 rounded-full bg-emerald-400 pulse-glow" />
                </div>
                {dp.addresses.length > 0 && (
                  <button
                    onClick={() => handlePairDiscovered(dp.addresses[0])}
                    className="mt-2 w-full px-3 py-1.5 text-sm bg-emerald-600/80 text-white rounded-lg hover:bg-emerald-500 transition-colors"
                  >
                    Pair
                  </button>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Paired Peers Section */}
      <div className="flex justify-between items-center mb-6">
        <h2 className="text-2xl font-bold text-white">Paired Peers</h2>
        <button
          onClick={() => {
            setPairAddress("");
            setShowPairForm(true);
          }}
          className="px-4 py-2 bg-gradient-to-r from-blue-600 to-purple-600 text-white rounded-lg hover:from-blue-500 hover:to-purple-500 transition-all"
        >
          Pair New Peer
        </button>
      </div>

      {peers.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-gray-400 mb-4">No peers paired yet</p>
          {discoveredPeers.length === 0 && (
            <p className="text-gray-500 text-sm mb-4">
              Start FileSync on another machine to discover it automatically,
              or enter an address manually.
            </p>
          )}
          <button
            onClick={() => {
              setPairAddress("");
              setShowPairForm(true);
            }}
            className="px-4 py-2 bg-gradient-to-r from-blue-600 to-purple-600 text-white rounded-lg hover:from-blue-500 hover:to-purple-500 transition-all"
          >
            Pair your first peer
          </button>
        </div>
      ) : (
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {peers.map((peer: PeerView) => {
            const isDiscovered = discoveredPeers.some((dp) => dp.id === peer.id);
            return (
              <div
                key={peer.id}
                className={`glass-card-hover p-4 ${
                  isDiscovered
                    ? "border-green-500/30 bg-gradient-to-br from-green-500/10 to-emerald-600/5"
                    : ""
                }`}
              >
                <div className="flex items-start justify-between mb-3">
                  <div className="flex-1">
                    <div className="flex items-center gap-2">
                      <h3 className="text-lg font-semibold text-white">{peer.name}</h3>
                      {isDiscovered && (
                        <div className="w-2.5 h-2.5 rounded-full bg-green-400 pulse-glow" title="Connected — Online" />
                      )}
                    </div>
                    <div className="mt-2 space-y-1 text-sm text-gray-400">
                      <p className="font-mono text-xs truncate text-gray-500" title={peer.fingerprint}>
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
                      {isDiscovered && (
                        <p className="text-xs text-green-400 font-medium">Online now</p>
                      )}
                    </div>
                  </div>
                  <div
                    className={`w-3 h-3 rounded-full ${
                      isDiscovered ? "bg-green-400 glow-green" : "bg-gray-600"
                    }`}
                    title={isDiscovered ? "Online" : "Offline"}
                  />
                </div>
                <button
                  onClick={() => handleUnpair(peer.id, peer.name)}
                  disabled={unpairingId === peer.id}
                  className="w-full px-3 py-1 text-sm border border-red-500/30 text-red-400 rounded-lg hover:bg-red-500/10 disabled:opacity-50 transition-colors"
                >
                  {unpairingId === peer.id ? "Removing..." : "Remove"}
                </button>
              </div>
            );
          })}
        </div>
      )}

      {showPairForm && (
        <PairForm
          onPaired={handlePaired}
          onCancel={() => setShowPairForm(false)}
          initialAddress={pairAddress}
        />
      )}
    </div>
  );
}
