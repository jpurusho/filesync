import { useState } from "react";
import { commands, PairingConfirmation } from "../lib/tauri";

interface PairFormProps {
  onPaired: () => void;
  onCancel: () => void;
}

export function PairForm({ onPaired, onCancel }: PairFormProps) {
  const [address, setAddress] = useState("");
  const [pairing, setPairing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<PairingConfirmation | null>(null);

  const handlePair = async () => {
    if (!address.trim()) {
      setError("Please enter a peer address (e.g., 192.168.1.10:8765)");
      return;
    }

    setPairing(true);
    setError(null);

    try {
      const result = await commands.pairPeer(address);
      setConfirmation(result);
      // Auto-close after showing fingerprint briefly
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
      <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
        <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4">
          <h3 className="text-xl font-bold text-green-600 mb-4">Pairing Successful!</h3>
          <div className="space-y-3">
            <div>
              <p className="text-sm text-gray-600">Peer Name:</p>
              <p className="font-semibold">{confirmation.peer_name}</p>
            </div>
            <div>
              <p className="text-sm text-gray-600">Peer ID:</p>
              <p className="font-mono text-xs">{confirmation.peer_id}</p>
            </div>
            <div>
              <p className="text-sm text-gray-600">Fingerprint (verify out-of-band):</p>
              <p className="font-mono text-sm bg-gray-100 p-2 rounded break-all">
                {confirmation.peer_fingerprint}
              </p>
            </div>
          </div>
          <button
            onClick={onPaired}
            className="mt-6 w-full px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700"
          >
            Done
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <h3 className="text-xl font-bold text-gray-900 mb-4">Pair New Peer</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Peer Address
            </label>
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="192.168.1.10:8765"
              disabled={pairing}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <p className="text-xs text-gray-500 mt-1">
              Format: IP:PORT (e.g., 192.168.1.10:8765)
            </p>
          </div>

          {error && (
            <div className="bg-red-50 border border-red-200 text-red-700 px-3 py-2 rounded-lg text-sm">
              {error}
            </div>
          )}

          <div className="flex gap-3">
            <button
              onClick={onCancel}
              disabled={pairing}
              className="flex-1 px-4 py-2 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-50 disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              onClick={handlePair}
              disabled={pairing || !address.trim()}
              className="flex-1 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
            >
              {pairing ? "Pairing..." : "Pair"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
