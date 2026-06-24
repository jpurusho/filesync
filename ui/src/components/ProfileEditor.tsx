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
      alert(`Failed to save profile: ${error}`);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 z-50">
        <div className="glass-card p-6 max-w-3xl w-full">
          <p className="text-gray-400">Loading...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 z-50">
      <div className="glass-card p-6 max-w-3xl w-full max-h-[90vh] overflow-y-auto">
        <h3 className="text-xl font-semibold text-white mb-4">
          {profileId ? "Edit Profile" : "Create Profile"}
        </h3>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Profile Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50 placeholder-gray-500"
              required
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Peer <span className="text-gray-500 text-xs">(optional)</span>
            </label>
            <select
              value={peerId}
              onChange={(e) => {
                setPeerId(e.target.value);
                const peer = peers.find((p) => p.id === e.target.value);
                if (peer) setPeerName(peer.name);
                else setPeerName("");
              }}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50"
            >
              <option value="" className="bg-gray-900">No peer (configure later)</option>
              {peers.map((peer) => (
                <option key={peer.id} value={peer.id} className="bg-gray-900">
                  {peer.name}
                </option>
              ))}
            </select>
            {peers.length === 0 && (
              <p className="text-sm text-gray-500 mt-1">
                No peers paired yet. Add anchors now, pair later.
              </p>
            )}
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">Mode</label>
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value)}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50"
            >
              <option value="push" className="bg-gray-900">Push (local &rarr; remote)</option>
              <option value="pull" className="bg-gray-900">Pull (remote &rarr; local)</option>
              <option value="bidi" className="bg-gray-900">Bidirectional</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Conflict Policy
            </label>
            <select
              value={conflictPolicy}
              onChange={(e) => setConflictPolicy(e.target.value)}
              className="w-full px-3 py-2 bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50"
            >
              <option value="newer_wins" className="bg-gray-900">Newer Wins</option>
              <option value="local_wins" className="bg-gray-900">Local Wins</option>
              <option value="remote_wins" className="bg-gray-900">Remote Wins</option>
            </select>
          </div>

          <div className="flex items-center">
            <input
              type="checkbox"
              checked={deletePropagation}
              onChange={(e) => setDeletePropagation(e.target.checked)}
              className="h-4 w-4 rounded border-white/20 bg-white/5 text-blue-600 focus:ring-blue-500/50"
            />
            <label className="ml-2 block text-sm text-gray-300">
              Propagate deletions to peer
            </label>
          </div>

          <div>
            <div className="flex justify-between items-center mb-2">
              <label className="block text-sm font-medium text-gray-300">Anchors</label>
              <button
                type="button"
                onClick={handleAddAnchor}
                className="text-sm text-blue-400 hover:text-blue-300 transition-colors"
              >
                + Add Anchor
              </button>
            </div>

            {anchors.map((anchor, index) => (
              <div key={index} className="bg-white/5 border border-white/10 rounded-lg p-4 mb-3">
                <div className="flex justify-between items-center mb-3">
                  <span className="text-sm font-medium text-gray-300">
                    Anchor {index + 1}
                  </span>
                  {anchors.length > 1 && (
                    <button
                      type="button"
                      onClick={() => handleRemoveAnchor(index)}
                      className="text-sm text-red-400 hover:text-red-300 transition-colors"
                    >
                      Remove
                    </button>
                  )}
                </div>

                <div className="space-y-3">
                  <div>
                    <label className="block text-xs text-gray-400 mb-1">Local Path</label>
                    <input
                      type="text"
                      value={anchor.local_path}
                      onChange={(e) =>
                        handleAnchorChange(index, "local_path", e.target.value)
                      }
                      placeholder="~/Documents or /path/to/local/folder"
                      className="w-full px-3 py-2 text-sm bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50 placeholder-gray-600"
                      required
                    />
                  </div>

                  <div>
                    <label className="block text-xs text-gray-400 mb-1">Remote Path</label>
                    <input
                      type="text"
                      value={anchor.remote_path}
                      onChange={(e) =>
                        handleAnchorChange(index, "remote_path", e.target.value)
                      }
                      placeholder="~/Documents or /path/to/remote/folder"
                      className="w-full px-3 py-2 text-sm bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50 placeholder-gray-600"
                      required
                    />
                  </div>

                  <div>
                    <label className="block text-xs text-gray-400 mb-1">
                      Max Depth (levels)
                    </label>
                    <input
                      type="number"
                      value={anchor.max_depth}
                      onChange={(e) =>
                        handleAnchorChange(index, "max_depth", parseInt(e.target.value))
                      }
                      min="1"
                      className="w-full px-3 py-2 text-sm bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50"
                    />
                  </div>

                  <div className="flex items-center">
                    <input
                      type="checkbox"
                      checked={anchor.include_hidden}
                      onChange={(e) =>
                        handleAnchorChange(index, "include_hidden", e.target.checked)
                      }
                      className="h-4 w-4 rounded border-white/20 bg-white/5 text-blue-600 focus:ring-blue-500/50"
                    />
                    <label className="ml-2 block text-xs text-gray-400">
                      Include hidden files
                    </label>
                  </div>

                  <div>
                    <label className="block text-xs text-gray-400 mb-1">
                      Ignore Patterns (one per line)
                    </label>
                    <textarea
                      value={anchor.ignore_patterns.join("\n")}
                      onChange={(e) => handleIgnorePatternsChange(index, e.target.value)}
                      placeholder={"node_modules\n*.log\n.git"}
                      rows={3}
                      className="w-full px-3 py-2 text-sm bg-white/5 border border-white/10 text-white rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500/50 font-mono placeholder-gray-600"
                    />
                  </div>
                </div>
              </div>
            ))}
          </div>

          <div className="flex justify-end gap-3 pt-4 border-t border-white/10">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 border border-white/10 text-gray-300 rounded-lg hover:bg-white/5 transition-colors"
              disabled={saving}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-2 bg-gradient-to-r from-blue-600 to-purple-600 text-white rounded-lg hover:from-blue-500 hover:to-purple-500 disabled:opacity-50 transition-all"
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
