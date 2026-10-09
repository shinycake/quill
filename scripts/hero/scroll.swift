// Smooth trackpad-style scroll at the current pointer position.
// usage: scroll <dy points (+ = content down / reveal above)> <seconds>
import CoreGraphics
import Foundation
let total = Double(CommandLine.arguments[1]) ?? 300
let secs = Double(CommandLine.arguments[2]) ?? 0.8
let steps = max(Int(secs * 120), 2)
func ease(_ u: Double) -> Double { u < 0.5 ? 4*u*u*u : 1 - pow(-2*u + 2, 3)/2 }
func post(_ dy: Int32, phase: Int64) {
    let e = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: dy, wheel2: 0, wheel3: 0)!
    e.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
    e.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase)
    e.post(tap: .cghidEventTap)
}
post(0, phase: 1) // began
var done = 0.0
for i in 1...steps {
    let target = total * ease(Double(i) / Double(steps))
    let dy = Int32((target - done).rounded())
    done += Double(dy)
    post(dy, phase: 2) // changed
    usleep(UInt32(secs / Double(steps) * 1_000_000))
}
post(0, phase: 4) // ended
