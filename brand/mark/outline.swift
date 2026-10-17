// Print one Grenze Gotisch 800 letter as an SVG path: 150 units, centred
// at x=100 on its advance, baseline at y=152 (the product-mark tile).
// swift outline.swift <font.ttf> <letter>
import Foundation
import CoreText
import CoreGraphics

let source = URL(fileURLWithPath: CommandLine.arguments[1])
let descriptors = CTFontManagerCreateFontDescriptorsFromURL(source as CFURL)! as! [CTFontDescriptor]
let descriptor = CTFontDescriptorCreateCopyWithVariation(descriptors[0], NSNumber(value: 0x77676874), 800)
let font = CTFontCreateWithFontDescriptor(descriptor, 150, nil)
var character = UniChar(CommandLine.arguments[2].utf16.first!)
var glyph: CGGlyph = 0
precondition(CTFontGetGlyphsForCharacters(font, &character, &glyph, 1))
var advance = CGSize.zero
CTFontGetAdvancesForGlyphs(font, .horizontal, &glyph, &advance, 1)
var transform = CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 100 - advance.width / 2, ty: 152)
let path = CTFontCreatePathForGlyph(font, glyph, &transform)!
var commands: [String] = []
func point(_ p: CGPoint) -> String { String(format: "%.2f %.2f", p.x, p.y) }
path.applyWithBlock { element in
    let e = element.pointee
    switch e.type {
    case .moveToPoint: commands.append("M" + point(e.points[0]))
    case .addLineToPoint: commands.append("L" + point(e.points[0]))
    case .addQuadCurveToPoint: commands.append("Q" + point(e.points[0]) + " " + point(e.points[1]))
    case .addCurveToPoint: commands.append("C" + point(e.points[0]) + " " + point(e.points[1]) + " " + point(e.points[2]))
    case .closeSubpath: commands.append("Z")
    @unknown default: fatalError("Unknown path element")
    }
}
print(commands.joined())
