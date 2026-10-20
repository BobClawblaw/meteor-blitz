#!/bin/bash
# Verify whether the Xbox controller is usable on this Mac.
#
# Checks, in order:
#   1. Is the controller enumerated at all (USB or Bluetooth)?
#   2. Does the game-controller driver accept it? (GameControllerSupportedHIDDevice)
#   3. Is it streaming HID reports? (InputReportCount must increase)
#   4. Does a real app see a usable extended gamepad with live values?
#
# Run this AFTER a fix (firmware update / re-pair / cable swap).
# A healthy controller: step 2 = Yes, step 3 increasing, step 4 count >= 1.
set -u

VENDOR_HEX=0x45e          # Microsoft
VENDOR_DEC=1118

echo "=== 1. Controller present? ==="
COUNT=$(hidutil list --matching "{\"VendorID\":$VENDOR_DEC}" 2>/dev/null | grep -c "$VENDOR_HEX")
if [ "$COUNT" -eq 0 ]; then
  echo "  NOT FOUND. Plug in (USB) or connect (Bluetooth), then re-run."
  exit 1
fi
hidutil list --matching "{\"VendorID\":$VENDOR_DEC}" 2>/dev/null | grep "$VENDOR_HEX" | awk '{print "  "$0}'

echo ""
echo "=== 2. GameController driver acceptance (want: Yes) ==="
ioreg -l -w 0 2>/dev/null | grep -i "GameControllerSupportedHIDDevice" | sort | uniq -c | sed 's/^/  /'

echo ""
echo "=== 3. HID report stream (InputReportCount must GROW between samples) ==="
ioreg -l -w 0 2>/dev/null | grep -o '"InputReportCount"=*[0-9]*' | sort -u > /tmp/mb_pad_a.txt
sleep 2
ioreg -l -w 0 2>/dev/null | grep -o '"InputReportCount"=*[0-9]*' | sort -u > /tmp/mb_pad_b.txt
echo "  sample A:"; sed 's/^/    /' /tmp/mb_pad_a.txt
echo "  sample B:"; sed 's/^/    /' /tmp/mb_pad_b.txt
if diff -q /tmp/mb_pad_a.txt /tmp/mb_pad_b.txt >/dev/null; then
  echo "  -> STALLED (no new reports). Controller linked but not sending input."
else
  echo "  -> STREAMING (report counts changed)."
fi

echo ""
echo "=== 4. App sees a usable gamepad? (want: count >= 1 with non-zero sticks/buttons) ==="
# Build a throwaway app bundle that prints what GCController exposes.
PROBE=/tmp/mb_gc_verify.app
if [ ! -x "$PROBE/Contents/MacOS/mb_gc_verify" ]; then
  mkdir -p "$PROBE/Contents/MacOS"
  cat > /tmp/mb_gc_verify.m <<'EOF'
#import <Foundation/Foundation.h>
#import <GameController/GameController.h>
static const char* on(BOOL b){ return b?"yes":"no"; }
int main(void){ @autoreleasepool {
  NSArray<GCController*>* cs = [GCController controllers];
  NSLog(@"count=%lu", (unsigned long)cs.count);
  for (GCController* c in cs) {
    GCExtendedGamepad* gp = c.extendedGamepad;
    NSLog(@"extPad=%@", gp ? @"yes" : @"NO");
    if (gp) NSLog(@"A=%s ls=(%.2f,%.2f) rs=(%.2f,%.2f) dpadUp=%s",
      on(gp.buttonA.isPressed),
      gp.leftThumbstick.xAxis.value, gp.leftThumbstick.yAxis.value,
      gp.rightThumbstick.xAxis.value, gp.rightThumbstick.yAxis.value,
      on(gp.dpad.up.isPressed));
  }
}}
EOF
  clang -framework Foundation -framework GameController \
    -o "$PROBE/Contents/MacOS/mb_gc_verify" /tmp/mb_gc_verify.m
  cat > "$PROBE/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>mb_gc_verify</string>
  <key>CFBundleExecutable</key><string>mb_gc_verify</string>
  <key>CFBundleIdentifier</key><string>dev.meteorblitz.verify</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>GCSupportsControllerUserInteraction</key><true/>
</dict></plist>
EOF
  codesign --force --sign - "$PROBE" >/dev/null 2>&1 || true
fi
open -a "$PROBE"
sleep 3
OUT=$(log show --predicate 'process == "mb_gc_verify"' --last 15s --style compact 2>/dev/null | grep -E "count=|extPad=|A=")
if [ -z "$OUT" ]; then
  echo "  NO OUTPUT — the app did not enumerate any controller."
else
  echo "$OUT" | sed 's/^/  /'
fi

echo ""
echo "=== Verdict ==="
GCYES=$(ioreg -l -w 0 2>/dev/null | grep -i "GameControllerSupportedHIDDevice" | grep -ci "= Yes")
if [ "${GCYES:-0}" -ge 1 ]; then
  echo "  PASS: macOS accepts the controller as a game controller."
  echo "        Now run the game: tools/make-app.sh  then  open target/MeteorBlitz.app"
  echo "        (MB_PAD_LOG=1 is set in the bundle; watch stderr for 'controller:' + 'pad:' lines)"
else
  echo "  FAIL: still GameControllerSupportedHIDDevice=No. Try: firmware update (Xbox Accessories on"
  echo "        Windows), then re-pair over Bluetooth (Forget first), then re-run this script."
fi
