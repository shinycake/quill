// Makes `sunset-hdr.heic`: a 960x640 dusk sky whose sun reaches 6x SDR
// white, stored as an SDR base plus an ISO gain map.
import CoreImage
import CoreImage.CIFilterBuiltins
import Foundation
let out = CommandLine.arguments[1]
let lin = CGColorSpace(name: CGColorSpace.extendedLinearSRGB)!
let rect = CGRect(x: 0, y: 0, width: 960, height: 640)
let sky = CIFilter.linearGradient()
sky.point0 = CGPoint(x: 0, y: 640); sky.point1 = CGPoint(x: 0, y: 180)
sky.color0 = CIColor(red: 0.02, green: 0.04, blue: 0.16, alpha: 1, colorSpace: lin)!
sky.color1 = CIColor(red: 0.95, green: 0.38, blue: 0.10, alpha: 1, colorSpace: lin)!
let glow = CIFilter.radialGradient()
glow.center = CGPoint(x: 600, y: 230); glow.radius0 = 0; glow.radius1 = 260
glow.color0 = CIColor(red: 2.2, green: 1.2, blue: 0.4, alpha: 1, colorSpace: lin)!
glow.color1 = CIColor(red: 0, green: 0, blue: 0, alpha: 0, colorSpace: lin)!
let sun = CIFilter.radialGradient()
sun.center = CGPoint(x: 600, y: 230); sun.radius0 = 34; sun.radius1 = 40
sun.color0 = CIColor(red: 6, green: 5.2, blue: 4, alpha: 1, colorSpace: lin)!
sun.color1 = CIColor(red: 0, green: 0, blue: 0, alpha: 0, colorSpace: lin)!
let ground = CIImage(color: CIColor(red: 0.015, green: 0.012, blue: 0.02, alpha: 1, colorSpace: lin)!).cropped(to: CGRect(x: 0, y: 0, width: 960, height: 180))
let add = { (a: CIImage, b: CIImage) -> CIImage in
  let f = CIFilter.additionCompositing(); f.inputImage = a; f.backgroundImage = b; return f.outputImage! }
var img = add(glow.outputImage!.cropped(to: rect), sky.outputImage!.cropped(to: rect))
img = sun.outputImage!.cropped(to: rect).composited(over: img)
img = ground.composited(over: img).cropped(to: rect)
let ctx = CIContext()
let p3 = CGColorSpace(name: CGColorSpace.displayP3)!
try ctx.writeHEIFRepresentation(of: img, to: URL(fileURLWithPath: out), format: .RGBA8, colorSpace: p3, options: [CIImageRepresentationOption.hdrImage: img, kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.8])
