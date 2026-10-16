import AppKit
import ScreenCaptureKit
import CoreVideo

// Usage: GlassRef <backdrop.png | backdrops-dir> dark|light [--capture outdir] [--hdrprobe]
// Keys: "b" next backdrop, "f" flip test against a window capture, "g" against a display capture, any other key stops the flip. Panels drag anywhere.
let args = CommandLine.arguments
let source = args.count > 1 ? args[1] : "sonoma.png"
let dark = args.count > 2 && args[2] == "dark"

var backdrops: [String] = {
    var isDir: ObjCBool = false
    if FileManager.default.fileExists(atPath: source, isDirectory: &isDir), isDir.boolValue {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: source)) ?? []
        return names.filter { $0.hasSuffix(".png") }.sorted().map { "\(source)/\($0)" }
    }
    return [source]
}()

class DragGlass: NSGlassEffectView {
    var last = NSPoint.zero
    override var mouseDownCanMoveWindow: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { frame.contains(point) ? self : nil }
    override func mouseDown(with e: NSEvent) { last = e.locationInWindow }
    override func mouseDragged(with e: NSEvent) {
        let p = e.locationInWindow
        setFrameOrigin(NSPoint(x: frame.origin.x + p.x - last.x, y: frame.origin.y + p.y - last.y))
        last = p
    }
}

class Root: NSView {
    var onKey: ((String) -> Void)?
    override var acceptsFirstResponder: Bool { true }
    override func keyDown(with e: NSEvent) { onKey?(e.charactersIgnoringModifiers ?? "") }
}

class Delegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var panels: [NSView] = []
    var imageView: NSImageView!
    var backdropIndex = 0
    var overlay: NSImageView?
    var flipLabel: NSTextField?
    var flipTimer: Timer?

    // Flip test: overlay a capture of this window on the live window and alternate every 0.5 s.
    // A faithful capture shows no change in the glass when it flips.
    @MainActor func startFlip(display: Bool) async {
        stopFlip()
        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard let w = content.windows.first(where: { $0.windowID == CGWindowID(window.windowNumber) }) else { return }
            let scale = window.backingScaleFactor
            let cfg = SCStreamConfiguration()
            cfg.showsCursor = false
            let filter: SCContentFilter
            let crop: CGRect
            let root = window.contentView!
            let contentOnScreen = window.convertToScreen(root.convert(root.bounds, to: nil))
            if display {
                guard let screen = window.screen,
                      let d = content.displays.first(where: { $0.displayID == (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID) }) else { return }
                filter = SCContentFilter(display: d, excludingWindows: [])
                cfg.width = Int(CGFloat(d.width) * scale); cfg.height = Int(CGFloat(d.height) * scale)
                let top = screen.frame.maxY - contentOnScreen.maxY
                crop = CGRect(x: (contentOnScreen.minX - screen.frame.minX) * scale, y: top * scale,
                              width: contentOnScreen.width * scale, height: contentOnScreen.height * scale)
            } else {
                filter = SCContentFilter(desktopIndependentWindow: w)
                cfg.width = Int(w.frame.width * scale); cfg.height = Int(w.frame.height * scale)
                let titlebar = w.frame.height - contentOnScreen.height
                crop = CGRect(x: 0, y: titlebar * scale, width: contentOnScreen.width * scale, height: contentOnScreen.height * scale)
            }
            let full = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: cfg)
            guard let img = full.cropping(to: crop) else { return }
            let ns = NSImage(cgImage: img, size: root.bounds.size)
            let iv = NSImageView(frame: root.bounds)
            iv.image = ns
            iv.imageScaling = .scaleNone
            root.addSubview(iv)
            let l = NSTextField(labelWithString: "")
            l.font = NSFont.boldSystemFont(ofSize: 14)
            l.textColor = .white
            l.backgroundColor = .black
            l.drawsBackground = true
            l.frame = NSRect(x: 6, y: 6, width: 260, height: 20)
            root.addSubview(l)
            overlay = iv; flipLabel = l
            let kind = display ? "DISPLAY" : "WINDOW"
            var showCapture = false
            flipTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
                showCapture.toggle()
                self?.overlay?.isHidden = !showCapture
                self?.flipLabel?.stringValue = showCapture ? "CAPTURE (\(kind))" : "LIVE"
            }
        } catch { print("flip failed: \(error)") }
    }
    func stopFlip() {
        flipTimer?.invalidate(); flipTimer = nil
        overlay?.removeFromSuperview(); overlay = nil
        flipLabel?.removeFromSuperview(); flipLabel = nil
    }

    func label(_ s: String, _ y: CGFloat) -> NSTextField {
        let t = NSTextField(labelWithString: s)
        t.font = NSFont.systemFont(ofSize: 13)
        t.frame = NSRect(x: 14, y: y, width: 220, height: 18)
        return t
    }
    func panel(_ style: NSGlassEffectView.Style, _ frame: NSRect, _ title: String, highlight: Int = -1) -> NSView {
        let g = DragGlass(frame: frame)
        g.style = style
        g.cornerRadius = 12
        let c = NSView(frame: NSRect(origin: .zero, size: frame.size))
        let items = [title, "Mute notifications", "View profile", "Set Wallpaper", "Archive chat", "Add to folder", "Auto-Delete", "Send a gift"]
        for (i, s) in items.enumerated() {
            if i == highlight {
                let h = NSView(frame: NSRect(x: 5, y: frame.height - 34 - CGFloat(i) * 26, width: frame.width - 10, height: 24))
                h.wantsLayer = true
                h.layer?.backgroundColor = NSColor.controlAccentColor.cgColor
                h.layer?.cornerRadius = 7
                c.addSubview(h)
            }
            c.addSubview(label(s, frame.height - 30 - CGFloat(i) * 26))
        }
        g.contentView = c
        return g
    }
    func showBackdrop(_ i: Int) {
        backdropIndex = (i + backdrops.count) % backdrops.count
        imageView.image = NSImage(contentsOfFile: backdrops[backdropIndex])
        window.title = "GlassRef: " + ((backdrops[backdropIndex] as NSString).lastPathComponent)
    }
    func windowCapture() async throws -> (SCContentFilter, SCWindow) {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let w = content.windows.first(where: { $0.windowID == CGWindowID(window.windowNumber) }) else {
            throw NSError(domain: "GlassRef", code: 1)
        }
        return (SCContentFilter(desktopIndependentWindow: w), w)
    }
    func capture(to path: String) async {
        do {
            let (filter, w) = try await windowCapture()
            let cfg = SCStreamConfiguration()
            cfg.width = Int(w.frame.width * 2); cfg.height = Int(w.frame.height * 2)
            cfg.showsCursor = false
            let img = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: cfg)
            try NSBitmapImageRep(cgImage: img).representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
            print("wrote \(path)")
        } catch { print("capture failed: \(error)") }
    }
    // Reads the window as float16 in SDR and in HDR (local display) and reports channel peaks.
    func hdrProbe() async {
        for (name, hdr) in [("sdr", false), ("hdr", true)] {
            do {
                let (filter, w) = try await windowCapture()
                let cfg = SCStreamConfiguration()
                cfg.width = Int(w.frame.width * 2); cfg.height = Int(w.frame.height * 2)
                cfg.showsCursor = false
                cfg.pixelFormat = kCVPixelFormatType_64RGBAHalf
                if hdr { cfg.captureDynamicRange = .hdrLocalDisplay }
                let sb = try await SCScreenshotManager.captureSampleBuffer(contentFilter: filter, configuration: cfg)
                guard let pb = CMSampleBufferGetImageBuffer(sb) else { print("\(name): no buffer"); continue }
                CVPixelBufferLockBaseAddress(pb, .readOnly)
                let wpx = CVPixelBufferGetWidth(pb), hpx = CVPixelBufferGetHeight(pb), row = CVPixelBufferGetBytesPerRow(pb)
                let base = CVPixelBufferGetBaseAddress(pb)!
                var peak: Float = 0, over = 0, peakAt = (0, 0)
                for y in 0..<hpx {
                    let p = (base + y * row).assumingMemoryBound(to: Float16.self)
                    for x in 0..<wpx {
                        let m = max(Float(p[x * 4]), Float(p[x * 4 + 1]), Float(p[x * 4 + 2]))
                        if m > 1.0 { over += 1 }
                        if m > peak { peak = m; peakAt = (x, y) }
                    }
                }
                CVPixelBufferUnlockBaseAddress(pb, .readOnly)
                let cs = CVImageBufferGetColorSpace(pb).map { "\($0.takeUnretainedValue())" } ?? "nil"
                print("\(name): \(wpx)x\(hpx) peak=\(peak) at \(peakAt) pixels>1.0=\(over) colorspace=\(cs)")
            } catch { print("\(name) failed: \(error)") }
        }
    }
    func applicationDidFinishLaunching(_ n: Notification) {
        NSApp.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 1200, height: 800),
                          styleMask: [.titled, .closable], backing: .buffered, defer: false)
        let root = Root(frame: NSRect(x: 0, y: 0, width: 1200, height: 800))
        imageView = NSImageView(frame: root.bounds)
        imageView.imageScaling = .scaleAxesIndependently
        root.addSubview(imageView)
        root.addSubview(panel(.regular, NSRect(x: 60, y: 300, width: 240, height: 230), "REGULAR"))
        root.addSubview(panel(.clear, NSRect(x: 340, y: 300, width: 240, height: 230), "CLEAR"))
        root.addSubview(panel(.regular, NSRect(x: 620, y: 300, width: 220, height: 230), "MENU-LIKE (Regular)", highlight: 3))
        root.onKey = { [weak self] k in
            guard let s = self else { return }
            switch k {
            case "b": s.stopFlip(); s.showBackdrop(s.backdropIndex + 1)
            case "f": Task { @MainActor in await s.startFlip(display: false) }
            case "g": Task { @MainActor in await s.startFlip(display: true) }
            default: s.stopFlip()
            }
        }
        window.contentView = root
        panels = root.subviews.filter { $0 is DragGlass }
        showBackdrop(0)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(root)
        NSApp.activate(ignoringOtherApps: true)
        if args.contains("--flip") || args.contains("--flipdisplay") {
            let display = args.contains("--flipdisplay")
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                await self.startFlip(display: display)
                try? await Task.sleep(nanoseconds: 700_000_000)
                await self.capture(to: "/tmp/glass-ref/flip-\(display ? "display" : "window").png")
                NSApp.terminate(nil)
            }
        } else if args.contains("--hdrprobe") {
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                await self.hdrProbe()
                NSApp.terminate(nil)
            }
        } else if let i = args.firstIndex(of: "--capture"), i + 1 < args.count {
            let out = args[i + 1]
            let spots: [(String, CGFloat, CGFloat)] = [
                ("pink", 120, 470), ("blue", 560, 480), ("green", 60, 120), ("lime", 380, 20),
                ("boundary", 200, 330), ("default", 60, 300)]
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                for b in 0..<backdrops.count {
                    self.showBackdrop(b)
                    let bname = ((backdrops[b] as NSString).lastPathComponent as NSString).deletingPathExtension
                    let dir = backdrops.count > 1 ? "\(out)/\(bname)" : out
                    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
                    for (name, x, y) in (bname == "wallpaper" || backdrops.count == 1 ? spots : [spots[5]]) {
                        for (k, p) in self.panels.enumerated() { p.setFrameOrigin(NSPoint(x: x + CGFloat(k) * 280, y: y)) }
                        try? await Task.sleep(nanoseconds: 700_000_000)
                        await self.capture(to: "\(dir)/apple-\(dark ? "dark" : "light")-\(name).png")
                    }
                }
                NSApp.terminate(nil)
            }
        }
    }
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let d = Delegate()
app.delegate = d
app.run()
