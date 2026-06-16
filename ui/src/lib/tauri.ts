import { invoke as tauriInvoke } from "@tauri-apps/api/core";

// Re-export with better typing
export const invoke = tauriInvoke;

// Type definitions for our commands
export interface ProfileView {
  id: string;
  name: string;
  mode: string;
  peer_name: string;
  delete_propagation: boolean;
  conflict_policy: string;
  updated_at: string;
  version: number;
  pending_deletion: boolean;
}

export interface ProfileDetail extends ProfileView {
  peer_id: string;
  created_at: string;
  origin_instance_id: string;
  anchors: AnchorView[];
}

export interface AnchorView {
  id: number;
  local_path: string;
  remote_path: string;
  max_depth: number;
  include_hidden: boolean;
  ignore_patterns: string[];
}

export interface PeerView {
  id: string;
  name: string;
  fingerprint: string;
  paired_at: string;
  last_seen: string | null;
  is_online: boolean;
}

export interface SyncStatus {
  profile_id: string;
  last_sync_at: string | null;
  last_sync_direction: string | null;
  files_synced: number | null;
  status: "idle" | "running" | "error";
  error_message: string | null;
}

export interface DriftSummary {
  profile_id: string;
  files_tracked: number;
  pending_local_changes: number;
  last_scan_at: string;
}

export interface ProfileInput {
  name: string;
  mode: string;
  peer_name: string;
  peer_id: string;
  delete_propagation: boolean;
  conflict_policy: string;
  anchors: AnchorInput[];
}

export interface AnchorInput {
  local_path: string;
  remote_path: string;
  max_depth: number;
  include_hidden: boolean;
  ignore_patterns: string[];
}

// Command wrappers
export const commands = {
  listProfiles: () => invoke<ProfileView[]>("list_profiles"),
  getProfile: (id: string) => invoke<ProfileDetail>("get_profile", { id }),
  createProfile: (input: ProfileInput) => invoke<ProfileView>("create_profile", { input }),
  updateProfile: (id: string, input: ProfileInput) => invoke<ProfileView>("update_profile", { id, input }),
  deleteProfile: (id: string) => invoke<void>("delete_profile", { id }),
  listPeers: () => invoke<PeerView[]>("list_peers"),
  listPendingDeletions: () => invoke<ProfileView[]>("list_pending_deletions"),
  confirmDeletion: (id: string) => invoke<void>("confirm_deletion", { id }),
  rejectDeletion: (id: string) => invoke<void>("reject_deletion", { id }),
  getSyncStatus: (profileId: string) => invoke<SyncStatus>("get_sync_status", { profileId }),
  getDriftSummary: (profileId: string) => invoke<DriftSummary>("get_drift_summary", { profileId }),
};
