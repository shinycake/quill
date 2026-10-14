// Makes the HDR viewer fixtures: `swift make-fixtures.swift <dir>`.
// gainmap.heic: 64x64, left half 0.5 grey, right half (4, 3, 2) in linear
//   extended sRGB, stored as an SDR base plus an ISO gain map.
// pq.heic: the same picture as 10-bit ITU-R 2100 PQ.
// sdr.heic: the grey half alone, no HDR data.
import CoreImage
import ImageIO
import Foundation
import UniformTypeIdentifiers
let out = CommandLine.arguments[1]
let lin = CGColorSpace(name: CGColorSpace.extendedLinearSRGB)!
// left half SDR grey ramp, right half bright 4x white
let bright = CIImage(color: CIColor(red: 4, green: 3, blue: 2, alpha: 1, colorSpace: lin)!).cropped(to: CGRect(x: 32, y: 0, width: 32, height: 64))
let dim = CIImage(color: CIColor(red: 0.5, green: 0.5, blue: 0.5, alpha: 1, colorSpace: lin)!).cropped(to: CGRect(x: 0, y: 0, width: 32, height: 64))
let hdr = bright.composited(over: dim)
let ctx = CIContext()
let p3 = CGColorSpace(name: CGColorSpace.displayP3)!
try ctx.writeHEIFRepresentation(of: hdr, to: URL(fileURLWithPath: out + "/gainmap.heic"), format: .RGBA8, colorSpace: p3, options: [CIImageRepresentationOption.hdrImage: hdr])
let sdr = dim
try ctx.writeHEIFRepresentation(of: sdr, to: URL(fileURLWithPath: out + "/sdr.heic"), format: .RGBA8, colorSpace: p3, options: [:])
// PQ 16-bit HEIC (no gain map)
let pq = CGColorSpace(name: CGColorSpace.itur_2100_PQ)!
try ctx.writeHEIF10Representation(of: hdr, to: URL(fileURLWithPath: out + "/pq.heic"), colorSpace: pq, options: [:])
