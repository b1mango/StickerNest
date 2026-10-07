import AppKit

let arguments = CommandLine.arguments
guard arguments.count >= 3, let image = NSImage(contentsOfFile: arguments[1]) else {
    fatalError("Usage: set-macos-icon.swift ICON TARGET [TARGET ...]")
}

// A Finder custom icon preserves the transparent silhouette on macOS Tahoe.
for path in arguments.dropFirst(2) {
    guard NSWorkspace.shared.setIcon(image, forFile: path, options: []) else {
        fatalError("Cannot set the custom icon: \(path)")
    }
    print("Applied transparent icon: \(path)")
}
