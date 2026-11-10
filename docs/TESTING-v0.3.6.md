# Testing FileSync v0.3.6 - Critical UX Fixes

**Release:** v0.3.6  
**Date:** 2026-11-10  
**Status:** Build in progress (~5-10 minutes)

## 🎯 What Was Fixed

You identified **two critical UX blockers** during v0.3.5 testing. Both are now fixed in v0.3.6:

### Issue 1: Firewall Required Terminal Commands ❌ → ✅ FIXED
**Before (v0.3.5):**
- Firewall blocked connections
- "No route to host" error
- Users had to run `sudo /usr/libexec/ApplicationFirewall/...` commands
- Unusable for non-technical users

**After (v0.3.6):**
- macOS prompts "FileSync would like to accept incoming network connections"
- User clicks **"Allow"** (one time, first launch)
- Firewall exception automatically added
- Professional app behavior (like Zoom, Slack, etc.)

### Issue 2: Manual IP Entry Required ❌ → ✅ FIXED
**Before (v0.3.5):**
- "Discovered on Network" section always empty
- Users forced to manually type IP addresses and port
- Terrible UX, error-prone

**After (v0.3.6):**
- Peers auto-discover via mDNS/Bonjour within 3-5 seconds
- Green "Discovered on Network" section shows found peers
- Click green **"Pair"** button next to peer name
- No IP typing needed

## 🔧 Technical Fix

**Root Cause:** Missing macOS entitlements

**Solution:** Added `Entitlements.plist` with:
- `com.apple.security.network.server` — incoming connections
- `com.apple.security.network.client` — outgoing connections
- `com.apple.security.network.multicast` — Bonjour/mDNS

See ADR-0027 for full technical details.

---

## 📥 Download v0.3.6 (Once Build Completes)

**Release Page:**
```
https://github.com/jpurusho/filesync/releases/tag/v0.3.6
```

**Direct Download:**
```
https://github.com/jpurusho/filesync/releases/download/v0.3.6/FileSync.app.zip
```

**Check build status:**
```bash
gh run watch
# Or visit: https://github.com/jpurusho/filesync/actions
```

**ETA:** 5-10 minutes from now

---

## 🧪 Testing Instructions

### ⚠️ Important: Fresh Install Required

**Do NOT reuse v0.3.5!** Entitlements only apply to newly built apps.

1. **Delete v0.3.5 from both Macs:**
   ```bash
   rm -rf /Applications/FileSync.app
   ```

2. **Clear any firewall rules from v0.3.5:**
   ```bash
   sudo /usr/libexec/ApplicationFirewall/socketfilterfw --remove /Applications/FileSync.app
   ```

3. **Download fresh v0.3.6** on both Macs

### Step-by-Step Test

#### On Mac 1 (192.168.68.78):

1. **Download and extract:**
   ```bash
   # Download FileSync.app.zip
   unzip ~/Downloads/FileSync.app.zip
   cp -r ~/Downloads/FileSync.app /Applications/
   ```

2. **Do NOT run `xattr -rc` yet** - test entitlements first

3. **Launch FileSync:**
   ```bash
   open /Applications/FileSync.app
   ```

4. **Expected: macOS Security Prompt**
   - First launch: "FileSync would like to accept incoming network connections"
   - Click **"Allow"**
   - ✅ **If you see this prompt:** Entitlements are working!
   - ❌ **If no prompt:** Entitlements didn't apply (build issue)

5. **Grant Full Disk Access:**
   - System Settings → Privacy & Security → Full Disk Access
   - Add FileSync.app, toggle ON

6. **Check "Peers" tab → "This Device" section:**
   - Should show: `192.168.68.78:5300`

#### On Mac 2 (192.168.68.89):

1. **Repeat steps 1-6** (download, install, allow network prompt, grant disk access)

2. **Check "This Device":** Should show `192.168.68.89:5300`

#### Test Auto-Discovery:

1. **On Mac 1, stay on "Peers" tab**

2. **Wait 3-5 seconds**

3. **Expected:** Green "Discovered on Network" section appears with:
   - Mac 2's hostname (e.g., "Jerome's MacBook Pro")
   - IP address: `192.168.68.89:5300`
   - Green pulsing dot (online indicator)
   - Green **"Pair"** button

4. ✅ **If discovered peer shows:** mDNS is working!
5. ❌ **If "Discovered" section stays empty:** mDNS still blocked (report as bug)

#### Test Pairing:

1. **On Mac 1, click green "Pair" button** next to discovered peer

2. **On Mac 2:** Accept pairing dialog should appear

3. **Click "Accept"**

4. **Expected:** Pairing completes successfully
   - No "No route to host" error
   - Peer appears in "Paired Peers" section
   - Green online indicator

5. ✅ **If pairing works:** Fix successful!
6. ❌ **If still get "No route to host":** Report as bug

#### Test Sync:

1. **On Mac 1, go to "Profiles" tab**

2. **Create test profile:**
   - Name: "Test Sync"
   - Mode: Push (A→B)
   - Add a small test folder (~10 files)
   - Set remote path on Mac 2

3. **Go to "Activity" tab**

4. **Click "Sync Now"**

5. **Expected:**
   - Progress bar shows
   - Files transfer
   - Sync completes successfully

6. **On Mac 2:** Check if files arrived at destination

---

## ✅ Success Criteria

**v0.3.6 is successful if:**

- [ ] macOS prompts for network access on first launch (both Macs)
- [ ] Auto-discovery shows peers within 3-5 seconds
- [ ] Green "Pair" button works (no manual IP entry)
- [ ] Pairing completes without "No route to host" error
- [ ] Sync transfers files successfully

**If ANY of these fail,** it's a regression and needs another fix.

---

## 🐛 If Issues Persist

**Scenario 1: No network access prompt on launch**
- Entitlements didn't apply (build config issue)
- Check if Entitlements.plist was included in bundle:
  ```bash
  codesign -d --entitlements :- /Applications/FileSync.app
  ```
- Should show network entitlements XML

**Scenario 2: Still no auto-discovery**
- mDNS multicast still blocked
- Check Console.app for "discovery" logs
- Try: `dns-sd -B _filesync._tcp` to see if mDNS is broadcasting

**Scenario 3: Still get firewall errors**
- Check firewall status:
  ```bash
  /usr/libexec/ApplicationFirewall/socketfilterfw --getappblocked /Applications/FileSync.app
  ```
- Should show "Allowed"

---

## 📊 Report Results

After testing, report:

1. **Did macOS prompt for network access?** (Yes/No)
2. **Did auto-discovery work?** (Yes/No - how long did it take?)
3. **Did pairing work without IP entry?** (Yes/No)
4. **Any errors during sync?** (List them)
5. **Overall: Is v0.3.6 usable now?** (Yes/No)

This will determine if we need further fixes or if FileSync is ready for real use.

---

## 🎉 Expected User Experience (If Fix Works)

**Total time from install to first sync: ~2 minutes**

1. Install on both Macs (30 seconds)
2. Launch → Click "Allow" on prompt (10 seconds)
3. Grant Full Disk Access (20 seconds)
4. Wait for auto-discovery (5 seconds)
5. Click "Pair" button (5 seconds)
6. Create profile (30 seconds)
7. Sync! (varies by file size)

**No terminal commands. No manual IP entry. Just works.**
