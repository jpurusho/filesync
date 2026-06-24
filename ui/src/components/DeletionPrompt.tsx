import { commands, ProfileView } from "../lib/tauri";

interface DeletionPromptProps {
  profile: ProfileView;
  onResolved: () => void;
}

export function DeletionPrompt({ profile, onResolved }: DeletionPromptProps) {
  const handleConfirm = async () => {
    try {
      await commands.confirmDeletion(profile.id);
      onResolved();
    } catch (error) {
      alert(`Failed to confirm: ${error}`);
    }
  };

  const handleReject = async () => {
    try {
      await commands.rejectDeletion(profile.id);
      onResolved();
    } catch (error) {
      alert(`Failed to reject: ${error}`);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="glass-card p-6 max-w-md w-full mx-4 border-orange-500/30">
        <h3 className="text-xl font-bold text-orange-400 mb-4">Profile Deletion Request</h3>
        <div className="space-y-3 mb-6">
          <p className="text-gray-300">
            Your peer <strong className="text-white">{profile.peer_name}</strong> has deleted the profile:
          </p>
          <div className="bg-white/5 p-3 rounded-lg border border-white/10">
            <p className="font-semibold text-white">{profile.name}</p>
            <p className="text-sm text-gray-400">Mode: {profile.mode}</p>
          </div>
          <p className="text-sm text-gray-400">
            Do you want to delete this profile locally as well?
          </p>
        </div>

        <div className="flex gap-3">
          <button
            onClick={handleReject}
            className="flex-1 px-4 py-2 border border-white/10 text-gray-300 rounded-lg hover:bg-white/5 transition-colors"
          >
            Keep Local Copy
          </button>
          <button
            onClick={handleConfirm}
            className="flex-1 px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-500 transition-colors"
          >
            Delete Locally
          </button>
        </div>
      </div>
    </div>
  );
}
