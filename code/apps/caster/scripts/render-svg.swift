// Rasterizes an SVG to a square PNG with WebKit, so building the icon needs
// nothing beyond the macOS SDK.
//
//   swift scripts/render-svg.swift <in.svg> <out.png> <pixels>
//   (`make icon` compiles it once with swiftc and runs the binary per size.)

import AppKit
import WebKit

let args = CommandLine.arguments
guard args.count == 4, let pixels = Int(args[3]) else {
    FileHandle.standardError.write("usage: render-svg.swift <in.svg> <out.png> <pixels>\n".data(using: .utf8)!)
    exit(2)
}
let input = URL(fileURLWithPath: args[1]).standardizedFileURL
let output = URL(fileURLWithPath: args[2])
let svg = try String(contentsOf: input, encoding: .utf8)

final class Renderer: NSObject, WKNavigationDelegate {
    let webView: WKWebView
    let size: CGFloat
    let output: URL

    init(size: CGFloat, output: URL) {
        self.size = size
        self.output = output
        webView = WKWebView(frame: NSRect(x: 0, y: 0, width: size, height: size))
        webView.setValue(false, forKey: "drawsBackground")
        super.init()
        webView.navigationDelegate = self
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        let config = WKSnapshotConfiguration()
        config.rect = NSRect(x: 0, y: 0, width: size, height: size)
        config.snapshotWidth = NSNumber(value: Double(size))
        webView.takeSnapshot(with: config) { image, error in
            // Redraw into a bitmap of exactly the requested pixel size,
            // whatever the display's backing scale was.
            let px = Int(self.size)
            guard let image,
                  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px,
                                             bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
                                             isPlanar: false, colorSpaceName: .deviceRGB,
                                             bytesPerRow: 0, bitsPerPixel: 0) else {
                FileHandle.standardError.write("snapshot failed: \(String(describing: error))\n".data(using: .utf8)!)
                exit(1)
            }
            rep.size = NSSize(width: px, height: px)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
            NSGraphicsContext.current?.imageInterpolation = .high
            image.draw(in: NSRect(x: 0, y: 0, width: px, height: px))
            NSGraphicsContext.restoreGraphicsState()
            guard let png = rep.representation(using: .png, properties: [:]) else {
                FileHandle.standardError.write("PNG encoding failed\n".data(using: .utf8)!)
                exit(1)
            }
            do {
                try png.write(to: self.output)
                exit(0)
            } catch {
                FileHandle.standardError.write("write failed: \(error)\n".data(using: .utf8)!)
                exit(1)
            }
        }
    }
}

let points = CGFloat(pixels)
let renderer = Renderer(size: points, output: output)
let html = """
<!doctype html><html><head><style>
html, body { margin: 0; background: transparent; }
svg { display: block; width: \(points)px; height: \(points)px; }
</style></head><body>\(svg)</body></html>
"""
renderer.webView.loadHTMLString(html, baseURL: input.deletingLastPathComponent())
RunLoop.main.run()
