import { create } from "zustand";
import { ProfileView, PeerView, SyncStatus, commands } from "./lib/tauri";

interface AppState {
  // Profiles
  profiles: ProfileView[];
  loadingProfiles: boolean;
  fetchProfiles: () => Promise<void>;

  // Peers
  peers: PeerView[];
  loadingPeers: boolean;
  fetchPeers: () => Promise<void>;

  // Sync status
  syncStatuses: Record<string, SyncStatus>;
  fetchSyncStatus: (profileId: string) => Promise<void>;

  // Pending deletions
  pendingDeletions: ProfileView[];
  fetchPendingDeletions: () => Promise<void>;

  // UI state
  activeTab: "profiles" | "peers" | "activity";
  setActiveTab: (tab: "profiles" | "peers" | "activity") => void;
}

export const useStore = create<AppState>((set) => ({
  // Profiles
  profiles: [],
  loadingProfiles: false,
  fetchProfiles: async () => {
    set({ loadingProfiles: true });
    try {
      const profiles = await commands.listProfiles();
      set({ profiles });
    } catch (error) {
      console.error("Failed to fetch profiles:", error);
    } finally {
      set({ loadingProfiles: false });
    }
  },

  // Peers
  peers: [],
  loadingPeers: false,
  fetchPeers: async () => {
    set({ loadingPeers: true });
    try {
      const peers = await commands.listPeers();
      set({ peers });
    } catch (error) {
      console.error("Failed to fetch peers:", error);
    } finally {
      set({ loadingPeers: false });
    }
  },

  // Sync status
  syncStatuses: {},
  fetchSyncStatus: async (profileId: string) => {
    try {
      const status = await commands.getSyncStatus(profileId);
      set((state: AppState) => ({
        syncStatuses: { ...state.syncStatuses, [profileId]: status },
      }));
    } catch (error) {
      console.error("Failed to fetch sync status:", error);
    }
  },

  // Pending deletions
  pendingDeletions: [],
  fetchPendingDeletions: async () => {
    try {
      const pendingDeletions = await commands.listPendingDeletions();
      set({ pendingDeletions });
    } catch (error) {
      console.error("Failed to fetch pending deletions:", error);
    }
  },

  // UI state
  activeTab: "profiles",
  setActiveTab: (tab: "profiles" | "peers" | "activity") => set({ activeTab: tab }),
}));
