// Records one window with ScreenCaptureKit: window pixels only (no other
// windows, no cursor, no shadow), at 60 fps and the display's pixel scale.
// usage: sckrec <pid> <seconds> <out.mov>
import AVFoundation
import AppKit
import Foundation
import ScreenCaptureKit

let args = CommandLine.arguments
guard args.count == 4, let pid = Int32(args[1]), let seconds = Double(args[2]) else {
    FileHandle.standardError.write("usage: sckrec <pid> <seconds> <out.mov>\n".data(using: .utf8)!)
    exit(2)
}
let outURL = URL(fileURLWithPath: args[3])
try? FileManager.default.removeItem(at: outURL)

final class Delegate: NSObject, SCRecordingOutputDelegate, SCStreamDelegate {
    func recordingOutputDidStartRecording(_ recordingOutput: SCRecordingOutput) {
        print("recording started \(Date().timeIntervalSince1970)")
        fflush(stdout)
    }
    func recordingOutput(_ recordingOutput: SCRecordingOutput, didFailWithError error: Error) {
        print("recording failed: \(error)")
        exit(1)
    }
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        print("stream stopped: \(error)")
        exit(1)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let delegate = Delegate()
Task {
    do {
        // Wait for the window to appear.
        var window: SCWindow?
        for _ in 0..<100 {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
            window = content.windows.first {
                $0.owningApplication?.processID == pid && $0.frame.width > 400 && $0.isOnScreen
            }
            if window != nil { break }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        guard let window else { print("window not found"); exit(1) }
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let config = SCStreamConfiguration()
        let scale = CGFloat(filter.pointPixelScale)
        config.width = Int(window.frame.width * scale)
        config.height = Int(window.frame.height * scale)
        config.minimumFrameInterval = CMTime(value: 1, timescale: 60)
        config.showsCursor = false
        config.ignoreShadowsSingleWindow = true
        config.capturesAudio = false
        config.queueDepth = 8
        config.colorSpaceName = CGColorSpace.sRGB
        let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
        let recConfig = SCRecordingOutputConfiguration()
        recConfig.outputURL = outURL
        recConfig.outputFileType = .mov
        recConfig.videoCodecType = .hevc
        let recording = SCRecordingOutput(configuration: recConfig, delegate: delegate)
        try stream.addRecordingOutput(recording)
        try await stream.startCapture()
        print("capturing \(config.width)x\(config.height) window \(window.windowID)")
        fflush(stdout)
        try await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
        try await stream.stopCapture()
        // Give the writer a moment to finalize the file.
        try await Task.sleep(nanoseconds: 800_000_000)
        print("done")
        exit(0)
    } catch {
        print("error: \(error)")
        exit(1)
    }
}
RunLoop.main.run()
