import AppKit
let W = 2400, H = 1600
func make(_ name: String, _ draw: (CGContext) -> Void) {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    draw(ctx)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "backdrops/\(name).png"))
}
func fill(_ c: CGContext, _ r: CGFloat, _ g: CGFloat, _ b: CGFloat) { c.setFillColor(red: r, green: g, blue: b, alpha: 1) }
make("grid") { c in fill(c, 1, 1, 1); c.fill(CGRect(x: 0, y: 0, width: W, height: H)); fill(c, 0.1, 0.1, 0.1)
    for x in stride(from: 0, to: W, by: 16) { c.fill(CGRect(x: x, y: 0, width: 2, height: H)) }
    for y in stride(from: 0, to: H, by: 16) { c.fill(CGRect(x: 0, y: y, width: W, height: 2)) } }
make("checker") { c in for y in stride(from: 0, to: H, by: 32) { for x in stride(from: 0, to: W, by: 32) {
    let on = ((x / 32) + (y / 32)) % 2 == 0; fill(c, on ? 0.95 : 0.15, on ? 0.95 : 0.15, on ? 0.95 : 0.15); c.fill(CGRect(x: x, y: y, width: 32, height: 32)) } } }
make("stripes") { c in let cols: [(CGFloat, CGFloat, CGFloat)] = [(0.9,0.2,0.2),(0.95,0.8,0.2),(0.2,0.7,0.3),(0.2,0.4,0.9),(0.6,0.3,0.8),(0.98,0.98,0.98),(0.05,0.05,0.05)]
    for (i, y) in stride(from: 0, to: H, by: 24).enumerated() { let k = cols[i % cols.count]; fill(c, k.0, k.1, k.2); c.fill(CGRect(x: 0, y: y, width: W, height: 24)) } }
make("diagonal") { c in fill(c, 0.95, 0.3, 0.35); c.fill(CGRect(x: 0, y: 0, width: W, height: H)); fill(c, 0.15, 0.35, 0.9)
    c.beginPath(); c.move(to: CGPoint(x: 0, y: 0)); c.addLine(to: CGPoint(x: W, y: 0)); c.addLine(to: CGPoint(x: W, y: H)); c.closePath(); c.fillPath() }
make("text") { c in fill(c, 1, 1, 1); c.fill(CGRect(x: 0, y: 0, width: W, height: H))
    let ns = NSGraphicsContext(cgContext: c, flipped: false); NSGraphicsContext.current = ns
    let s = String(repeating: "The quick brown fox jumps over the lazy dog 0123456789. Liquid glass bends light at its rim. ", count: 4)
    for (i, y) in stride(from: 0, to: H, by: 36).enumerated() {
        let col: NSColor = [.black, .systemBlue, .systemRed, .systemGreen][i % 4]
        (s as NSString).draw(at: NSPoint(x: -CGFloat(i * 37 % 300), y: CGFloat(y)), withAttributes: [.font: NSFont.systemFont(ofSize: 26), .foregroundColor: col]) } }
