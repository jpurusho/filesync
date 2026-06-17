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
      console.error("Failed to confirm deletion:", error);
      alert(`Failed to confirm: ${error}`);
    }
  };

  const handleReject = async () => {
    try {
      await commands.rejectDeletion(profile.id);
      onResolved();
    } catch (error) {
      console.error("Failed to reject deletion:", error);
      alert(`Failed to reject: ${error}`);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <h3 className="text-xl font-bold text-orange-600 mb-4">Profile Deletion Request</h3>
        <div className="space-y-3 mb-6">
          <p className="text-gray-700">
            Your peer <strong>{profile.peer_name}</strong> has deleted the profile:
          </p>
          <div className="bg-gray-100 p-3 rounded">
            <p className="font-semibold">{profile.name}</p>
            <p className="text-sm text-gray-600">Mode: {profile.mode}</p>
          </div>
          <p className="text-sm text-gray-600">
            Do you want to delete this profile locally as well?
          </p>
        </div>

        <div className="flex gap-3">
          <button
            onClick={handleReject}
            className="flex-1 px-4 py-2 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-50"
          >
            Keep Local Copy
          </button>
          <button
            onClick={handleConfirm}
            className="flex-1 px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700"
          >
            Delete Locally
          </button>
        </div>
      </div>
    </div>
  );
}
