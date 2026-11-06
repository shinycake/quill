// Outside-in launch timer for scripts/launch-bench.py --onscreen.
//
//     winwatch OWNER OUTFILE [TIMEOUT_S]
//
// Polls WindowServer's on-screen window list every 2 ms and writes
// `onscreen <unix µs>` to OUTFILE when a normal-level window owned by the
// process named OWNER (taller than 100 points) is first on screen. Reading
// owners and bounds needs no Screen Recording permission.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: winwatch OWNER OUTFILE [TIMEOUT_S]\n".data(using: .utf8)!)
    exit(2)
}
let owner = args[1]
let out = args[2]
let timeout = args.count > 3 ? Double(args[3]) ?? 20 : 20
let start = Date()

func onScreen() -> Bool {
    let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
    guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
        return false
    }
    return list.contains { window in
        let name = window[kCGWindowOwnerName as String] as? String
        let layer = window[kCGWindowLayer as String] as? Int
        let bounds = window[kCGWindowBounds as String] as? [String: Any]
        let height = bounds?["Height"] as? Double ?? 0
        return name == owner && layer == 0 && height > 100
    }
}

while Date().timeIntervalSince(start) < timeout {
    if onScreen() {
        let micros = Int64(Date().timeIntervalSince1970 * 1_000_000)
        try? "onscreen \(micros)\n".write(toFile: out, atomically: true, encoding: .utf8)
        exit(0)
    }
    usleep(2000)
}
exit(1)
