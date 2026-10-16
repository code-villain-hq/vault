import Foundation

// Generated from color.json.
enum ColorTokens {
  enum Palette {
    static let night = "#0A0B0D"
    static let cloak = "#5A1470"
    static let cloakHover = "#6B1884"
    static let poison = "#C9E265"
    static let poisonDeep = "#4C6A00"
    static let wash = "#1A0D24"
    static let ink = "#ECECF1"
    static let muted = "#9B9BA8"
    static let panel = "#12131A"
    static let line = "#2A2B36"
  }

  struct Theme {
    let bg: String
    let panel: String
    let surface: String
    let hover: String
    let selected: String
    let line: String
    let fg: String
    let muted: String
    let faint: String
    let primary: String
    let primaryHover: String
    let onPrimary: String
    let accent: String
    let danger: String
    let ok: String
    let warn: String
    let info: String
  }

  static let night = Theme(
    bg: "#0A0B0D",
    panel: "#12131A",
    surface: "#191A23",
    hover: "#1B1C26",
    selected: "#1A0D24",
    line: "#2A2B36",
    fg: "#ECECF1",
    muted: "#9B9BA8",
    faint: "#868693",
    primary: "#331C40",
    primaryHover: "#3A1A49",
    onPrimary: "#ECECF1",
    accent: "#C9E265",
    danger: "#F27A6B",
    ok: "#5CC9A7",
    warn: "#E9AE45",
    info: "#7AAEF5"
  )

  static let day = Theme(
    bg: "#F5F5F7",
    panel: "#FFFFFF",
    surface: "#F0F0F2",
    hover: "#EAEAED",
    selected: "#F3EAF7",
    line: "#E3E3E7",
    fg: "#18181C",
    muted: "#55555D",
    faint: "#67676E",
    primary: "#E8DEEB",
    primaryHover: "#DBCBE0",
    onPrimary: "#18181C",
    accent: "#4C6A00",
    danger: "#B5382C",
    ok: "#1B7357",
    warn: "#8A5A00",
    info: "#2F62B8"
  )
}
