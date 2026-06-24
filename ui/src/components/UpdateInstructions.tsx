interface UpdateInstructionsProps {
  onClose: () => void;
}

export function UpdateInstructions({ onClose }: UpdateInstructionsProps) {
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="glass-card p-6 max-w-2xl mx-4 border-blue-500/30">
        <div className="flex items-start gap-3">
          <div className="flex-shrink-0 mt-1">
            <svg
              className="h-6 w-6 text-blue-400"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
          <div className="flex-1">
            <h3 className="text-lg font-semibold text-white mb-2">
              Update Downloaded Successfully!
            </h3>
            <p className="text-gray-300 mb-4">
              The update has been downloaded to your <strong>Downloads</strong> folder.
              Follow these steps to complete the installation:
            </p>

            <div className="space-y-4 bg-white/5 rounded-lg p-4 mb-4">
              <div className="flex gap-3">
                <div className="flex-shrink-0 w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-xs font-semibold text-white">
                  1
                </div>
                <div className="flex-1">
                  <p className="text-gray-200 font-medium">Open the downloaded DMG file</p>
                  <p className="text-sm text-gray-400 mt-1">
                    Find <code className="px-1.5 py-0.5 bg-black/30 rounded text-gray-300">FileSync_*.dmg</code> in your Downloads folder
                  </p>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-xs font-semibold text-white">
                  2
                </div>
                <div className="flex-1">
                  <p className="text-gray-200 font-medium">Drag FileSync.app to Applications</p>
                  <p className="text-sm text-gray-400 mt-1">
                    Replace the existing app when prompted
                  </p>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-xs font-semibold text-white">
                  3
                </div>
                <div className="flex-1">
                  <p className="text-gray-200 font-medium">Remove quarantine attribute (one-time)</p>
                  <p className="text-sm text-gray-400 mt-1 mb-2">
                    Open Terminal and run this command:
                  </p>
                  <div className="flex items-center gap-2">
                    <code className="flex-1 px-3 py-2 bg-black/40 rounded text-gray-300 font-mono text-sm">
                      xattr -cr /Applications/FileSync.app
                    </code>
                    <button
                      onClick={() => copyToClipboard("xattr -cr /Applications/FileSync.app")}
                      className="px-3 py-2 bg-white/10 hover:bg-white/20 rounded text-xs text-gray-300 transition-colors"
                      title="Copy to clipboard"
                    >
                      Copy
                    </button>
                  </div>
                  <p className="text-xs text-yellow-400/80 mt-2 flex items-start gap-1">
                    <svg className="w-4 h-4 flex-shrink-0 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                      <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
                    </svg>
                    <span>This step is required because the app is not yet code-signed. Future versions will be signed and won't need this.</span>
                  </p>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-xs font-semibold text-white">
                  4
                </div>
                <div className="flex-1">
                  <p className="text-gray-200 font-medium">Quit and restart FileSync</p>
                  <p className="text-sm text-gray-400 mt-1">
                    Close this app completely (⌘Q) and launch it again from Applications
                  </p>
                </div>
              </div>
            </div>

            <div className="bg-blue-500/10 border border-blue-500/30 rounded-lg p-3 mb-4">
              <p className="text-sm text-blue-200">
                <strong>Alternative:</strong> Right-click FileSync.app → Open (first launch only).
                This bypasses Gatekeeper for unsigned apps.
              </p>
            </div>

            <div className="flex justify-end">
              <button
                onClick={onClose}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition-colors font-medium"
              >
                Got it!
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
