import Foundation

// StickerNest bookmark helper — store and resolve security-scoped bookmarks so
// the app can keep access to the user's WeChat container after one confirmation.
// @_cdecl gives C-callable symbols for the Rust side; ~60 lines by design.

@discardableResult
private func writeOut(_ text: String, _ out: UnsafeMutablePointer<CChar>, _ outLen: Int32) -> Int32 {
    let bytes = [UInt8](text.utf8)
    guard bytes.count + 1 <= outLen else { return 0 }
    for i in 0..<bytes.count {
        out[i] = CChar(bitPattern: bytes[i])
    }
    out[bytes.count] = 0
    return Int32(bytes.count)
}

@_cdecl("sn_bookmark_store")
public func sn_bookmark_store(
    _ path: UnsafePointer<CChar>,
    _ out: UnsafeMutablePointer<CChar>,
    _ outLen: Int32
) -> Int32 {
    let pathText = String(cString: path)
    let url = URL(fileURLWithPath: pathText)
    guard let data = try? url.bookmarkData(options: .withSecurityScope,
                                          includingResourceValuesForKeys: nil,
                                          relativeTo: nil) else { return 0 }
    return writeOut(data.base64EncodedString(), out, outLen)
}

@_cdecl("sn_bookmark_resolve")
public func sn_bookmark_resolve(
    _ base64: UnsafePointer<CChar>,
    _ out: UnsafeMutablePointer<CChar>,
    _ outLen: Int32
) -> Int32 {
    let text = String(cString: base64)
    guard let data = Data(base64Encoded: text) else { return 0 }
    var stale = false
    guard let url = try? URL(resolvingBookmarkData: data,
                             options: .withSecurityScope,
                             relativeTo: nil,
                             bookmarkDataIsStale: &stale),
          !stale else { return 0 }
    if !url.startAccessingSecurityScopedResource() { return 0 }
    return writeOut(url.path, out, outLen)
}
