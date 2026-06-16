import { useEffect, useState } from "react";
import { useStore } from "../store";
import { ProfileCard } from "../components/ProfileCard";
import { ProfileEditor } from "../components/ProfileEditor";
import { commands, ProfileView } from "../lib/tauri";

export function ProfilesPage() {
  const { profiles, loadingProfiles, fetchProfiles } = useStore();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [editingProfileId, setEditingProfileId] = useState<string | undefined>(undefined);

  useEffect(() => {
    fetchProfiles();
  }, [fetchProfiles]);

  const handleDelete = async (id: string) => {
    if (confirm("Delete this profile?")) {
      try {
        await commands.deleteProfile(id);
        await fetchProfiles();
      } catch (error) {
        console.error("Failed to delete profile:", error);
        alert("Failed to delete profile");
      }
    }
  };

  const handleEdit = (id: string) => {
    setEditingProfileId(id);
  };

  const handleCloseEditor = () => {
    setShowCreateModal(false);
    setEditingProfileId(undefined);
  };

  const handleSave = () => {
    fetchProfiles();
  };

  if (loadingProfiles) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-gray-500">Loading profiles...</p>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="flex justify-between items-center mb-6">
        <h2 className="text-2xl font-bold text-gray-900">Sync Profiles</h2>
        <button
          onClick={() => setShowCreateModal(true)}
          className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
        >
          Create Profile
        </button>
      </div>

      {profiles.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-gray-500 mb-4">No profiles yet</p>
          <button
            onClick={() => setShowCreateModal(true)}
            className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
          >
            Create your first profile
          </button>
        </div>
      ) : (
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {profiles.map((profile: ProfileView) => (
            <ProfileCard
              key={profile.id}
              profile={profile}
              onEdit={() => handleEdit(profile.id)}
              onDelete={() => handleDelete(profile.id)}
            />
          ))}
        </div>
      )}

      {showCreateModal && (
        <ProfileEditor onClose={handleCloseEditor} onSave={handleSave} />
      )}

      {editingProfileId && (
        <ProfileEditor
          profileId={editingProfileId}
          onClose={handleCloseEditor}
          onSave={handleSave}
        />
      )}
    </div>
  );
}
