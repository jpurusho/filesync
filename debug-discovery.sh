#!/bin/bash
# FileSync Discovery Debug Script
# Run on BOTH Macs and compare results

echo "=========================================="
echo "FileSync Discovery Debug"
echo "=========================================="
echo ""

echo "Mac Information:"
echo "Hostname: $(hostname)"
echo "IP Address: $(ifconfig | grep "inet " | grep -v 127.0.0.1 | awk '{print $2}' | head -1)"
echo ""

echo "1. Check if FileSync is running:"
if pgrep -x "FileSync" > /dev/null; then
    echo "✓ FileSync is running (PID: $(pgrep -x FileSync))"
else
    echo "❌ FileSync is NOT running"
    echo "   Start it with: open /Applications/FileSync.app"
    exit 1
fi
echo ""

echo "2. Check if port 5300 is listening:"
PORT_CHECK=$(lsof -iTCP:5300 -sTCP:LISTEN)
if [ -n "$PORT_CHECK" ]; then
    echo "✓ Port 5300 is listening:"
    echo "$PORT_CHECK"
else
    echo "❌ Port 5300 is NOT listening"
    echo "   This means network services didn't start!"
fi
echo ""

echo "3. Check entitlements:"
ENTITLEMENTS=$(codesign -d --entitlements :- /Applications/FileSync.app 2>&1)
if echo "$ENTITLEMENTS" | grep -q "com.apple.security.network.multicast"; then
    echo "✓ Network multicast entitlement present"
else
    echo "❌ Network multicast entitlement missing!"
fi
echo ""

echo "4. Check mDNS broadcasting (_filesync._tcp):"
echo "   Searching for 5 seconds..."
timeout 5 dns-sd -B _filesync._tcp local. 2>&1 | grep -v "^Browsing" | grep -v "^DATE" | grep -v "Timestamp" || echo "❌ No FileSync services found via mDNS"
echo ""

echo "5. Check firewall status:"
FW_STATUS=$(/usr/libexec/ApplicationFirewall/socketfilterfw --getappblocked /Applications/FileSync.app 2>&1)
if echo "$FW_STATUS" | grep -q "Allowed"; then
    echo "✓ FileSync is allowed through firewall"
elif echo "$FW_STATUS" | grep -q "Blocked"; then
    echo "❌ FileSync is BLOCKED by firewall"
else
    echo "⚠️  Firewall status: $FW_STATUS"
fi
echo ""

echo "6. Check recent app logs (last 30 seconds):"
log show --predicate 'process == "FileSync"' --last 30s --info 2>/dev/null | grep -i "network\|discovery\|mdns\|listen\|error" | tail -10 || echo "No recent logs found"
echo ""

echo "7. Test network connectivity to other Mac:"
read -p "Enter other Mac's IP address (e.g., 192.168.68.89): " OTHER_IP
if [ -n "$OTHER_IP" ]; then
    echo "   Pinging $OTHER_IP..."
    if ping -c 2 -t 2 "$OTHER_IP" > /dev/null 2>&1; then
        echo "✓ Can ping $OTHER_IP"
    else
        echo "❌ Cannot ping $OTHER_IP"
    fi

    echo "   Testing port 5300 on $OTHER_IP..."
    if nc -z -w 2 "$OTHER_IP" 5300 2>&1 | grep -q "succeeded"; then
        echo "✓ Port 5300 is reachable on $OTHER_IP"
    else
        echo "❌ Port 5300 is NOT reachable on $OTHER_IP"
    fi
fi
echo ""

echo "=========================================="
echo "Debug Complete"
echo "=========================================="
echo ""
echo "Please send this output to diagnose the issue."
