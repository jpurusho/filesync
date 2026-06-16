import { ProfileView } from "../lib/tauri";

interface ProfileCardProps {
  profile: ProfileView;
  onEdit: () => void;
  onDelete: () => void;
}

export function ProfileCard({ profile, onEdit, onDelete }: ProfileCardProps) {
  return (
    <div className="border border-gray-200 rounded-lg p-4 hover:shadow-md transition-shadow">
      <div className="flex justify-between items-start">
        <div className="flex-1">
          <h3 className="text-lg font-semibold text-gray-900">{profile.name}</h3>
          <div className="mt-2 space-y-1 text-sm text-gray-600">
            <p>
              <span className="font-medium">Mode:</span>{" "}
              <span className="capitalize">{profile.mode}</span>
            </p>
            <p>
              <span className="font-medium">Peer:</span> {profile.peer_name || "None"}
            </p>
            <p>
              <span className="font-medium">Conflict Policy:</span>{" "}
              <span className="capitalize">{profile.conflict_policy}</span>
            </p>
            <p className="text-xs text-gray-500">
              Last updated: {new Date(profile.updated_at).toLocaleString()}
            </p>
          </div>
        </div>
        <div className="flex gap-2">
          <button
            onClick={onEdit}
            className="px-3 py-1 text-sm text-blue-600 hover:text-blue-800"
          >
            Edit
          </button>
          <button
            onClick={onDelete}
            className="px-3 py-1 text-sm text-red-600 hover:text-red-800"
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
