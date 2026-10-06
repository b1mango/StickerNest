#!/bin/sh
set -e
cd "$(dirname "$0")"
/usr/bin/xcrun swiftc -emit-library -O -module-name bookmark_helper \
  -target arm64-apple-macos13.0 \
  -o bookmark-helper.dylib bookmark.swift
echo built
/usr/bin/shasum -a 256 bookmark-helper.dylib | awk '{print "sha256:", $1}'
