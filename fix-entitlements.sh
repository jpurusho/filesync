#!/bin/bash
# FileSync Entitlements Fix Script
# Run this on BOTH Macs to manually apply network entitlements

set -e  # Exit on error

echo "=========================================="
echo "FileSync Entitlements Fix"
echo "=========================================="
echo ""

# Check if FileSync is installed
if [ ! -d "/Applications/FileSync.app" ]; then
    echo "❌ Error: FileSync.app not found in /Applications/"
    echo "   Please install FileSync first."
    exit 1
fi

echo "✓ Found FileSync.app"
echo ""

# Create temporary entitlements file
echo "📝 Creating entitlements file..."
cat > /tmp/FileSync.entitlements << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.network.server</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.network.multicast</key>
    <true/>
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>
</dict>
</plist>
EOF

echo "✓ Created /tmp/FileSync.entitlements"
echo ""

# Quit FileSync if running
echo "🛑 Checking if FileSync is running..."
if pgrep -x "FileSync" > /dev/null; then
    echo "   Quitting FileSync..."
    killall FileSync 2>/dev/null || true
    sleep 2
    echo "✓ FileSync quit"
else
    echo "✓ FileSync not running"
fi
echo ""

# Re-sign with entitlements
echo "🔏 Re-signing FileSync.app with entitlements..."
codesign --force --deep --sign - --entitlements /tmp/FileSync.entitlements /Applications/FileSync.app

if [ $? -eq 0 ]; then
    echo "✓ Re-signing successful"
else
    echo "❌ Re-signing failed"
    exit 1
fi
echo ""

# Verify entitlements
echo "✅ Verifying entitlements were applied..."
ENTITLEMENTS=$(codesign -d --entitlements :- /Applications/FileSync.app 2>&1)

if echo "$ENTITLEMENTS" | grep -q "com.apple.security.network.server"; then
    echo "✓ Network server entitlement: FOUND"
else
    echo "❌ Network server entitlement: NOT FOUND"
fi

if echo "$ENTITLEMENTS" | grep -q "com.apple.security.network.client"; then
    echo "✓ Network client entitlement: FOUND"
else
    echo "❌ Network client entitlement: NOT FOUND"
fi

if echo "$ENTITLEMENTS" | grep -q "com.apple.security.network.multicast"; then
    echo "✓ Network multicast entitlement: FOUND"
else
    echo "❌ Network multicast entitlement: NOT FOUND"
fi

echo ""
echo "=========================================="
echo "✅ Fix Applied Successfully!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "1. Launch FileSync: open /Applications/FileSync.app"
echo "2. You should see macOS prompt: 'Allow incoming connections?'"
echo "3. Click 'Allow'"
echo "4. Go to Peers tab and wait 5 seconds"
echo "5. You should see discovered peers appear"
echo ""
echo "If you don't see the network prompt, the fix didn't work."
echo "Check Console.app for errors."
echo ""
