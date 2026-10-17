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

  enum Opacity {
    static let statusTint: Double = 0.13
    static let chipTint: Double = 0.07
    static let highlightTint: Double = 0.08
  }

  enum Contrast {
    static let text: Double = 4.6
    static let signal: Double = 3
  }

  struct Theme {
    let bg: String
    let panel: String
    let surface: String
    let hover: String
    let line: String
    let primary: String
    let primaryHover: String
    let selected: String
    let dialog: String
    let field: String
    let fieldHover: String
    let dialogField: String
    let dialogFieldHover: String
    let fg: String
    let muted: String
    let faint: String
    let onPrimary: String
    let accent: String
    let signal: String
    let danger: String
    let ok: String
    let warn: String
    let info: String
    let statusDangerInk: String
    let statusOkInk: String
    let statusWarnInk: String
    let statusInfoInk: String
    let statusNeutralInk: String
  }

  static let night = Theme(
    bg: "#0B0B0B",
    panel: "#141414",
    surface: "#1B1B1B",
    hover: "#1D1D1D",
    line: "#2B2B2B",
    primary: "#34183D",
    primaryHover: "#3B1846",
    selected: "#1B0D1F",
    dialog: "#1D1D1D",
    field: "#1B1B1B",
    fieldHover: "#1D1D1D",
    dialogField: "#141414",
    dialogFieldHover: "#1B1B1B",
    fg: "#EBEBEB",
    muted: "#9B9B9B",
    faint: "#969696",
    onPrimary: "#EBEBEB",
    accent: "#C9E265",
    signal: "#C9E265",
    danger: "#F27A6B",
    ok: "#5CC9A7",
    warn: "#E9AE45",
    info: "#7AAEF5",
    statusDangerInk: "#F27A6B",
    statusOkInk: "#5CC9A7",
    statusWarnInk: "#E9AE45",
    statusInfoInk: "#7AAEF5",
    statusNeutralInk: "#9B9B9B"
  )

  static let day = Theme(
    bg: "#F5F5F5",
    panel: "#FFFFFF",
    surface: "#F0F0F0",
    hover: "#EBEBEB",
    line: "#E1E1E1",
    primary: "#E8DEEB",
    primaryHover: "#DBCBE0",
    selected: "#F3EFF5",
    dialog: "#FFFFFF",
    field: "#F0F0F0",
    fieldHover: "#EBEBEB",
    dialogField: "#F0F0F0",
    dialogFieldHover: "#EBEBEB",
    fg: "#181818",
    muted: "#555555",
    faint: "#5E5E5E",
    onPrimary: "#181818",
    accent: "#607000",
    signal: "#7C9000",
    danger: "#B54338",
    ok: "#00775D",
    warn: "#8C6100",
    info: "#396AAB",
    statusDangerInk: "#A7362D",
    statusOkInk: "#006A53",
    statusWarnInk: "#7D5600",
    statusInfoInk: "#2E5E9E",
    statusNeutralInk: "#555555"
  )
}
