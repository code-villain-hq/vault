import Foundation

// Generated from type.json. App windows use the largest size of each fluid range.
enum TypeTokens {
  static let display = "Grenze Gotisch"
  static let reading = "Atkinson Hyperlegible Next"
  static let mono = "Atkinson Hyperlegible Mono"

  struct Role {
    let face: String
    let weight: Int
    let size: Double
    let lineHeight: Double
    let trackingEm: Double
  }

  static let splash = Role(face: display, weight: 800, size: 184, lineHeight: 0.95, trackingEm: 0)
  static let displayTitle = Role(face: display, weight: 700, size: 56, lineHeight: 1.1, trackingEm: 0)
  static let headline = Role(face: display, weight: 700, size: 34, lineHeight: 1.18, trackingEm: 0)
  static let title = Role(face: reading, weight: 700, size: 21, lineHeight: 1.25, trackingEm: 0)
  static let label = Role(face: reading, weight: 700, size: 15, lineHeight: 1.2, trackingEm: 0.01)
  static let body = Role(face: reading, weight: 400, size: 17, lineHeight: 1.6, trackingEm: 0)
  static let small = Role(face: reading, weight: 400, size: 14, lineHeight: 1.5, trackingEm: 0)
  static let code = Role(face: mono, weight: 400, size: 14, lineHeight: 1.6, trackingEm: 0)
  static let ui = Role(face: reading, weight: 400, size: 14, lineHeight: 1.45, trackingEm: 0)
  static let uiSmall = Role(face: reading, weight: 400, size: 13, lineHeight: 1.4, trackingEm: 0)
  static let control = Role(face: reading, weight: 600, size: 13, lineHeight: 1, trackingEm: 0)
  static let uiCode = Role(face: mono, weight: 400, size: 13, lineHeight: 1.6, trackingEm: 0)
}
