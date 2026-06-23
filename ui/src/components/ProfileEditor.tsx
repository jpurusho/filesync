import { useState, useEffect } from "react";
import { commands, ProfileInput, AnchorInput } from "../lib/tauri";

interface ProfileEditorProps {
  profileId?: string;
  onClose: () => void;
  onSave: () => void;
}

export function ProfileEditor({ profileId, onClose, onSave }: ProfileEditorProps) {
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [name, setName] = useState("");
  const [mode, setMode] = useState("push");
  const [peerName, setPeerName] = useState("");
  const [peerId, setPeerId] = useState("");
  const [deletePropagation, setDeletePropagation] = useState(true);
  const [conflictPolicy, setConflictPolicy] = useState("newer_wins");
  const [anchors, setAnchors] = useState<AnchorInput[]>([
    {
      local_path: "",
      remote_path: "",
      max_depth: 100,
      include_hidden: false,
      ignore_patterns: [],
    },
  ]);
  const [peers, setPeers] = useState<Array<{ id: string; name: string }>>([]);

  useEffect(() => {
    async function loadData() {
      try {
        const peerList = await commands.listPeers();
        setPeers(peerList.map((p) => ({ id: p.id, name: p.name })));

        if (profileId) {
          setLoading(true);
          const profile = await commands.getProfile(profileId);
          setName(profile.name);
          setMode(profile.mode);
          setPeerName(profile.peer_name);
          setPeerId(profile.peer_id);
          setDeletePropagation(profile.delete_propagation);
          setConflictPolicy(profile.conflict_policy);
          setAnchors(
            profile.anchors.length > 0
              ? profile.anchors.map((a) => ({
                  local_path: a.local_path,
                  remote_path: a.remote_path,
                  max_depth: a.max_depth,
                  include_hidden: a.include_hidden,
                  ignore_patterns: a.ignore_patterns,
                }))
              : [
                  {
                    local_path: "",
                    remote_path: "",
                    max_depth: 100,
                    include_hidden: false,
                    ignore_patterns: [],
                  },
                ]
          );
        }
      } catch (error) {
        console.error("Failed to load profile:", error);
        alert("Failed to load profile");
      } finally {
        setLoading(false);
      }
    }
    loadData();
  }, [profileId]);

  const handleAddAnchor = () => {
    setAnchors([
      ...anchors,
      {
        local_path: "",
        remote_path: "",
        max_depth: 100,
        include_hidden: false,
        ignore_patterns: [],
      },
    ]);
  };

  const handleRemoveAnchor = (index: number) => {
    setAnchors(anchors.filter((_, i) => i !== index));
  };

  const handleAnchorChange = (index: number, field: keyof AnchorInput, value: any) => {
    const updated = [...anchors];
    updated[index] = { ...updated[index], [field]: value };
    setAnchors(updated);
  };

  const handleIgnorePatternsChange = (index: number, value: string) => {
    const patterns = value.split("\n").filter((p) => p.trim().length > 0);
    handleAnchorChange(index, "ignore_patterns", patterns);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (anchors.length === 0) {
      alert("At least one anchor is required");
      return;
    }

    const emptyAnchors = anchors.filter((a) => !a.local_path || !a.remote_path);
    if (emptyAnchors.length > 0) {
      alert("All anchors must have local and remote paths");
      return;
    }

    const input: ProfileInput = {
      name,
      mode,
      peer_name: peerName,
      peer_id: peerId,
      delete_propagation: deletePropagation,
      conflict_policy: conflictPolicy,
      anchors,
    };

    try {
      setSaving(true);
      if (profileId) {
        await commands.updateProfile(profileId, input);
      } else {
        await commands.createProfile(input);
      }
      onSave();
      onClose();
    } catch (error) {
      console.error("Failed to save profile:", error);
      alert(`Failed to save profile: ${error}`);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50">
        <div className="bg-white rounded-lg p-6 max-w-3xl w-full max-h-[90vh] overflow-y-auto">
          <p className="text-gray-500">Loading...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50">
      <div className="bg-white rounded-lg p-6 max-w-3xl w-full max-h-[90vh] overflow-y-auto">
        <h3 className="text-xl font-semibold mb-4">
          {profileId ? "Edit Profile" : "Create Profile"}
        </h3>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Profile Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              required
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Peer <span className="text-gray-400 text-xs">(optional - can be set later)</span>
            </label>
            <select
              value={peerId}
              onChange={(e) => {
                setPeerId(e.target.value);
                const peer = peers.find((p) => p.id === e.target.value);
                if (peer) setPeerName(peer.name);
                else setPeerName("");
              }}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="">No peer (configure later)</option>
              {peers.map((peer) => (
                <option key={peer.id} value={peer.id}>
                  {peer.name}
                </option>
              ))}
            </select>
            {peers.length === 0 && (
              <p className="text-sm text-gray-500 mt-1">
                No peers paired yet. You can add anchors now and pair later.
              </p>
            )}
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">Mode</label>
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="push">Push (local → remote)</option>
              <option value="pull">Pull (remote → local)</option>
              <option value="bidi">Bidirectional</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Conflict Policy
            </label>
            <select
              value={conflictPolicy}
              onChange={(e) => setConflictPolicy(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="newer_wins">Newer Wins</option>
              <option value="local_wins">Local Wins</option>
              <option value="remote_wins">Remote Wins</option>
            </select>
          </div>

          <div className="flex items-center">
            <input
              type="checkbox"
              checked={deletePropagation}
              onChange={(e) => setDeletePropagation(e.target.checked)}
              className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
            />
            <label className="ml-2 block text-sm text-gray-700">
              Propagate deletions to peer
            </label>
          </div>

          <div>
            <div className="flex justify-between items-center mb-2">
              <label className="block text-sm font-medium text-gray-700">Anchors</label>
              <button
                type="button"
                onClick={handleAddAnchor}
                className="text-sm text-blue-600 hover:text-blue-700"
              >
                + Add Anchor
              </button>
            </div>

            {anchors.map((anchor, index) => (
              <div key={index} className="border border-gray-200 rounded-md p-4 mb-3">
                <div className="flex justify-between items-center mb-3">
                  <span className="text-sm font-medium text-gray-700">
                    Anchor {index + 1}
                  </span>
                  {anchors.length > 1 && (
                    <button
                      type="button"
                      onClick={() => handleRemoveAnchor(index)}
                      className="text-sm text-red-600 hover:text-red-700"
                    >
                      Remove
                    </button>
                  )}
                </div>

                <div className="space-y-3">
                  <div>
                    <label className="block text-xs text-gray-600 mb-1">Local Path</label>
                    <input
                      type="text"
                      value={anchor.local_path}
                      onChange={(e) =>
                        handleAnchorChange(index, "local_path", e.target.value)
                      }
                      placeholder="~/Documents or /path/to/local/folder"
                      className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                      required
                    />
                  </div>

                  <div>
                    <label className="block text-xs text-gray-600 mb-1">Remote Path</label>
                    <input
                      type="text"
                      value={anchor.remote_path}
                      onChange={(e) =>
                        handleAnchorChange(index, "remote_path", e.target.value)
                      }
                      placeholder="~/Documents or /path/to/remote/folder"
                      className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                      required
                    />
                  </div>

                  <div>
                    <label className="block text-xs text-gray-600 mb-1">
                      Max Depth (levels)
                    </label>
                    <input
                      type="number"
                      value={anchor.max_depth}
                      onChange={(e) =>
                        handleAnchorChange(index, "max_depth", parseInt(e.target.value))
                      }
                      min="1"
                      className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                    />
                  </div>

                  <div className="flex items-center">
                    <input
                      type="checkbox"
                      checked={anchor.include_hidden}
                      onChange={(e) =>
                        handleAnchorChange(index, "include_hidden", e.target.checked)
                      }
                      className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
                    />
                    <label className="ml-2 block text-xs text-gray-600">
                      Include hidden files
                    </label>
                  </div>

                  <div>
                    <label className="block text-xs text-gray-600 mb-1">
                      Ignore Patterns (one per line)
                    </label>
                    <textarea
                      value={anchor.ignore_patterns.join("\n")}
                      onChange={(e) => handleIgnorePatternsChange(index, e.target.value)}
                      placeholder="node_modules&#10;*.log&#10;.git"
                      rows={3}
                      className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
                    />
                  </div>
                </div>
              </div>
            ))}
          </div>

          <div className="flex justify-end gap-3 pt-4 border-t">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 bg-gray-200 text-gray-700 rounded-md hover:bg-gray-300"
              disabled={saving}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              disabled={saving}
            >
              {saving ? "Saving..." : profileId ? "Update" : "Create"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
