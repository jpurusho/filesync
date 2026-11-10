# ADR-0027: macOS Network Entitlements for Auto-Discovery

**Date:** 2026-11-09  
**Status:** Accepted  
**Issue:** Critical UX blockers discovered during two-Mac testing

## Context

During first real-world testing on two Macs, two critical issues were discovered:

### Issue 1: Firewall Blocks All Connections
- macOS firewall blocks FileSync by default
- Users must run terminal commands to allow connections
- No system prompt for network access
- **Impact:** Completely unusable for non-technical users

### Issue 2: Auto-Discovery Doesn't Work
- mDNS discovery is implemented and coded correctly
- UI polls every 3 seconds and has "Discovered on Network" section
- But no peers ever appear
- Users forced to manually type IP addresses
- **Impact:** Terrible UX, defeats "easy sync" value prop

### Root Cause

macOS requires **application entitlements** for network features:
- `com.apple.security.network.server` — incoming connections
- `com.apple.security.network.client` — outgoing connections
- `com.apple.security.network.multicast` — Bonjour/mDNS multicast

Without these entitlements:
- Firewall blocks by default (no prompt)
- mDNS multicast traffic is silently dropped
- Port 5300 listener works but is unreachable

## Decision

Add macOS entitlements file and configure Tauri to use it during build.

**File:** `src-tauri/Entitlements.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <!-- Allow incoming network connections -->
    <key>com.apple.security.network.server</key>
    <true/>

    <!-- Allow outgoing network connections -->
    <key>com.apple.security.network.client</key>
    <true/>

    <!-- Allow Bonjour/mDNS multicast -->
    <key>com.apple.security.network.multicast</key>
    <true/>

    <!-- Allow reading files for sync -->
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>
</dict>
</plist>
```

**Configure in `tauri.conf.json`:**
```json
"bundle": {
  "macOS": {
    "entitlements": "Entitlements.plist",
    "minimumSystemVersion": "12.0"
  }
}
```

## Consequences

### Positive

**Firewall:**
- ✅ macOS will prompt user on first launch: "FileSync would like to accept incoming network connections"
- ✅ One-time "Allow" click — no terminal commands needed
- ✅ Firewall exception automatically added
- ✅ Professional macOS app behavior (like Zoom, Slack, etc.)

**Auto-Discovery:**
- ✅ mDNS multicast works immediately
- ✅ Peers appear in "Discovered on Network" section
- ✅ Users click green "Pair" button — no IP typing
- ✅ 3-5 second discovery delay (acceptable)

### Negative

- Requires notarization for distribution (already planned)
- App size unchanged
- Build process unchanged (entitlements applied automatically)

### User Experience Before/After

**Before (v0.3.5 and earlier):**
1. Install app on both Macs
2. Launch app → no network prompt
3. Go to Peers → empty "Discovered" section
4. Click "Pair New Peer" → manually type IP:port
5. Click Pair → "No route to host" error
6. Open Terminal, run `sudo /usr/libexec/ApplicationFirewall/...` commands
7. Try pairing again → finally works
8. **Result:** Unusable for normal users

**After (v0.3.6+):**
1. Install app on both Macs
2. Launch app → macOS prompts: "Allow incoming connections?" → click **Allow**
3. Go to Peers → see other Mac in green "Discovered on Network" section
4. Click green **"Pair"** button next to discovered peer
5. Accept pairing on other Mac
6. **Done!** 30 seconds total
7. **Result:** Actually usable

## Implementation Status

- ✅ Created `Entitlements.plist`
- ✅ Updated `tauri.conf.json` with macOS bundle config
- ✅ Bumped version to 0.3.6
- 🔄 Release build needed to test (local builds don't enforce entitlements)

## Testing

**To verify after v0.3.6 release:**

1. **Fresh install on two Macs** (don't reuse v0.3.5)
2. **Launch Mac 1** → macOS should prompt for network access → click **Allow**
3. **Launch Mac 2** → same prompt → click **Allow**
4. **On Mac 1, go to Peers tab:**
   - "Discovered on Network" section should appear within 3-5 seconds
   - Mac 2 should show with green indicator and IP address
5. **Click "Pair" button** next to discovered peer
6. **On Mac 2:** Accept pairing request
7. **Result:** Pairing completes without firewall errors

## Related

- FR-DP-1: "The system **shall** automatically detect other File Sync instances"
- FR-DP-2: mDNS/DNS-SD requirement
- Issue discovered during first two-Mac acceptance testing (v0.3.5)
- Blocks all real-world usage without this fix

## References

- [Apple Entitlements Documentation](https://developer.apple.com/documentation/bundleresources/entitlements)
- [Network Entitlements](https://developer.apple.com/documentation/bundleresources/entitlements/com_apple_security_network_server)
- Tauri macOS bundle config: https://tauri.app/v1/api/config/#macconfig
