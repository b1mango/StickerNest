#!/bin/sh
# Build the StickerNest WeChat key-dumping dylib with the system clang.
# Output: wechat-key-dumper.dylib next to this script.
set -e
cd "$(dirname "$0")"
CLANG="$(xcrun --find clang)"
SDK="$(xcrun --show-sdk-path)"
"$CLANG" -dynamiclib -arch arm64 -isysroot "$SDK" \
  -framework Foundation -framework Security \
  -fvisibility=hidden \
  -O2 -Wall -Wextra \
  -o wechat-key-dumper.dylib key_dumper.c fishhook.c
echo "built $(pwd)/wechat-key-dumper.dylib"
/usr/bin/shasum -a 256 wechat-key-dumper.dylib | awk '{print "sha256:", $1}'
