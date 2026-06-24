import { useEffect, useState } from "react";
import { ProfileView, DriftSummary, commands } from "../lib/tauri";

interface ProfileCardProps {
  profile: ProfileView;
  onEdit: () => void;
  onDelete: () => void;
}

export function ProfileCard({ profile, onEdit, onDelete }: ProfileCardProps) {
  const [drift, setDrift] = useState<DriftSummary | null>(null);

  useEffect(() => {
    commands.getDriftSummary(profile.id).then(setDrift).catch(console.error);
  }, [profile.id]);

  const modeColors: Record<string, string> = {
    push: "from-blue-500/20 to-blue-600/10 border-blue-500/30",
    pull: "from-green-500/20 to-green-600/10 border-green-500/30",
    bidi: "from-purple-500/20 to-purple-600/10 border-purple-500/30",
  };

  const modeLabel: Record<string, string> = {
    push: "Push",
    pull: "Pull",
    bidi: "Bidirectional",
  };

  return (
    <div className={`glass-card-hover p-4 bg-gradient-to-br ${modeColors[profile.mode] || modeColors.push}`}>
      <div className="flex justify-between items-start">
        <div className="flex-1">
          <h3 className="text-lg font-semibold text-white">{profile.name}</h3>
          <div className="mt-2 space-y-1.5 text-sm">
            <p className="text-gray-300">
              <span className="text-gray-500">Mode:</span>{" "}
              {modeLabel[profile.mode] || profile.mode}
            </p>
            <p className="text-gray-300">
              <span className="text-gray-500">Peer:</span>{" "}
              {profile.peer_name || <span className="text-gray-500 italic">None</span>}
            </p>
            <p className="text-gray-300">
              <span className="text-gray-500">Conflict:</span>{" "}
              <span className="capitalize">{profile.conflict_policy.replace("_", " ")}</span>
            </p>
            {drift && (
              <div className="text-xs bg-white/5 text-blue-300 px-2 py-1 rounded mt-2 inline-block">
                {drift.files_tracked} files tracked
                {drift.pending_local_changes > 0 && (
                  <span className="text-yellow-300 ml-1">
                    ({drift.pending_local_changes} pending)
                  </span>
                )}
              </div>
            )}
            <p className="text-xs text-gray-500">
              Updated: {new Date(profile.updated_at).toLocaleString()}
            </p>
          </div>
        </div>
        <div className="flex flex-col gap-1">
          <button
            onClick={onEdit}
            className="px-3 py-1 text-xs text-blue-300 hover:text-blue-200 hover:bg-white/5 rounded transition-colors"
          >
            Edit
          </button>
          <button
            onClick={onDelete}
            className="px-3 py-1 text-xs text-red-400 hover:text-red-300 hover:bg-white/5 rounded transition-colors"
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
