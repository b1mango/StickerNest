#!/bin/sh
set -e
cd "$(dirname "$0")"
/usr/bin/xcrun swiftc -O -target arm64-apple-macos13.0 -o ocr-helper ocr.swift
echo built
/usr/bin/shasum -a 256 ocr-helper | awk '{print "sha256:", $1}'
