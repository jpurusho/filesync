import { useState } from "react";
import { commands, PairingConfirmation } from "../lib/tauri";

interface PairFormProps {
  onPaired: () => void;
  onCancel: () => void;
  initialAddress?: string;
}

export function PairForm({ onPaired, onCancel, initialAddress = "" }: PairFormProps) {
  const [address, setAddress] = useState(initialAddress);
  const [pairing, setPairing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<PairingConfirmation | null>(null);

  const handlePair = async () => {
    if (!address.trim()) {
      setError("Please enter a peer address (e.g., 192.168.1.10:5300)");
      return;
    }

    setPairing(true);
    setError(null);

    try {
      const result = await commands.pairPeer(address);
      setConfirmation(result);
      setTimeout(() => {
        onPaired();
      }, 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPairing(false);
    }
  };

  if (confirmation) {
    return (
      <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50">
        <div className="glass-card p-6 max-w-md w-full mx-4 border-green-500/30">
          <h3 className="text-xl font-bold text-green-400 mb-4">Pairing Successful!</h3>
          <div className="space-y-3">
            <div>
              <p className="text-sm text-gray-400">Peer Name:</p>
              <p className="font-semibold text-white">{confirmation.peer_name}</p>
            </div>
            <div>
              <p className="text-sm text-gray-400">Peer ID:</p>
              <p className="font-mono text-xs text-gray-300">{confirmation.peer_id}</p>
            </div>
            <div>
              <p className="text-sm text-gray-400">Fingerprint (verify out-of-band):</p>
              <p className="font-mono text-sm bg-white/5 p-2 rounded text-green-300 break-all">
                {confirmation.peer_fingerprint}
              </p>
            </div>
          </div>
          <button
            onClick={onPaired}
            className="mt-6 w-full px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-500 transition-colors"
          >
            Done
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="glass-card p-6 max-w-md w-full mx-4">
        <h3 className="text-xl font-bold text-white mb-4">Pair New Peer</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Peer Address
            </label>
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="192.168.1.10:5300"
              disabled={pairing}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50 placeholder-gray-500"
            />
            <p className="text-xs text-gray-500 mt-1">
              Use the address shown in the peer's "This Device" section
            </p>
          </div>

          {error && (
            <div className="bg-red-500/10 border border-red-500/30 text-red-300 px-3 py-2 rounded-lg text-sm">
              {error}
            </div>
          )}

          <div className="flex gap-3">
            <button
              onClick={onCancel}
              disabled={pairing}
              className="flex-1 px-4 py-2 border border-white/10 text-gray-300 rounded-lg hover:bg-white/5 disabled:opacity-50 transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handlePair}
              disabled={pairing || !address.trim()}
              className="flex-1 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-500 disabled:opacity-40 transition-colors"
            >
              {pairing ? "Pairing..." : "Pair"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
