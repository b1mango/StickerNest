import Foundation
import Vision
// On-device text recognition (Apple Vision) for character rules such as 龙图.
// Prints one JSON line per input image: {"path": ..., "text": ...}; unreadable
// images yield empty text. Only the first frame of animations is read.
for path in CommandLine.arguments.dropFirst() {
    var text = ""
    if let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
       let image = CGImageSourceCreateImageAtIndex(src, 0, nil) {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["zh-Hans", "en-US"]
        request.usesLanguageCorrection = false
        try? VNImageRequestHandler(cgImage: image).perform([request])
        text = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: " ")
    }
    let data = try! JSONSerialization.data(withJSONObject: ["path": path, "text": text])
    print(String(data: data, encoding: .utf8)!)
}
