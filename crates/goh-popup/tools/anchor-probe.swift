// S-04 spike: can the macOS Accessibility API give a trustworthy caret position in a terminal?
//
// Run it INSIDE the terminal you want to test (it needs Accessibility permission for that app:
// System Settings > Privacy & Security > Accessibility), at a normal shell prompt:
//
//   swift crates/goh-popup/tools/anchor-probe.swift --note "iTerm2, single pane, 1 monitor" --out iterm-single.json
//
// It switches to the alternate screen, writes known text, places the cursor on characters and right
// after typed text with escape sequences, then restores the screen and cursor, asks the app for the caret rectangle after each move, and checks the
// rectangles against the terminal's own cell size (XTWINOPS 16t). A position is "anchored correctly"
// when the reported rectangle is within half a cell of where the cell grid says it must be, inside
// the window, and non-empty. docs/docs/05 go criterion: >= 98% across the corpus.
//
// Repeat for: iTerm2 and Terminal.app; one pane and split panes; one monitor and two monitors
// (window on each); a Retina and a non-Retina display if you have one. Send me the JSON files.

import Cocoa
import ApplicationServices

// MARK: - helpers

func attr(_ el: AXUIElement, _ name: String) -> CFTypeRef? {
    var v: CFTypeRef?
    return AXUIElementCopyAttributeValue(el, name as CFString, &v) == .success ? v : nil
}

func attrNames(_ el: AXUIElement) -> [String] {
    var a: CFArray?
    _ = AXUIElementCopyAttributeNames(el, &a)
    return (a as? [String]) ?? []
}

func paramNames(_ el: AXUIElement) -> [String] {
    var a: CFArray?
    _ = AXUIElementCopyParameterizedAttributeNames(el, &a)
    return (a as? [String]) ?? []
}

func axRange(_ v: CFTypeRef?) -> CFRange? {
    guard let v, CFGetTypeID(v) == AXValueGetTypeID() else { return nil }
    var r = CFRange()
    return AXValueGetValue(v as! AXValue, .cfRange, &r) ? r : nil
}

func axRect(_ v: CFTypeRef?) -> CGRect? {
    guard let v, CFGetTypeID(v) == AXValueGetTypeID() else { return nil }
    var r = CGRect.zero
    return AXValueGetValue(v as! AXValue, .cgRect, &r) ? r : nil
}

func bounds(_ el: AXUIElement, _ loc: Int, _ len: Int) -> CGRect? {
    var range = CFRange(location: loc, length: len)
    guard let param = AXValueCreate(.cfRange, &range) else { return nil }
    var out: CFTypeRef?
    guard AXUIElementCopyParameterizedAttributeValue(el, "AXBoundsForRange" as CFString, param, &out) == .success else { return nil }
    return axRect(out)
}

func rectJSON(_ r: CGRect?) -> Any {
    guard let r else { return NSNull() }
    return ["x": r.origin.x, "y": r.origin.y, "w": r.size.width, "h": r.size.height]
}

// MARK: - terminal queries (bounded, raw tty)

let tty = open("/dev/tty", O_RDWR)

func withRawTTY<T>(_ body: () -> T) -> T {
    var saved = termios()
    tcgetattr(tty, &saved)
    var raw = saved
    cfmakeraw(&raw)
    raw.c_cc.16 = 0 // VMIN
    raw.c_cc.17 = 1 // VTIME: 100 ms per read
    tcsetattr(tty, TCSANOW, &raw)
    defer { tcsetattr(tty, TCSANOW, &saved) }
    return body()
}

func send(_ s: String) { _ = s.withCString { write(tty, $0, strlen($0)) } }

/// Send a query and read the reply up to its terminator, within ~400 ms. Must be called in raw mode.
func query(_ seq: String, terminators: [UInt8]) -> String? {
    send(seq)
    var buf = [UInt8]()
    let deadline = Date().addingTimeInterval(0.4)
    var chunk = [UInt8](repeating: 0, count: 64)
    while Date() < deadline && buf.count < 128 {
        let n = read(tty, &chunk, chunk.count)
        if n > 0 { buf += chunk[0..<n] }
        if let l = buf.last, terminators.contains(l) { break }
    }
    return buf.isEmpty ? nil : String(decoding: buf, as: UTF8.self)
}

/// `ESC [ a ; b ; c t` -> [a, b, c]
func numbers(_ reply: String?) -> [Double]? {
    guard let r = reply, let start = r.range(of: "[") else { return nil }
    let body = r[start.upperBound...].dropLast() // strip final 't'
    let parts = body.split(separator: ";").compactMap { Double($0) }
    return parts.isEmpty ? nil : parts
}

// MARK: - main

func probe() -> Int32 {
    var note = "", out: String?
    var args = CommandLine.arguments.dropFirst().makeIterator()
    while let a = args.next() {
        if a == "--note" { note = args.next() ?? "" }
        if a == "--out" { out = args.next() }
    }

    // Asking (prompt: true) makes macOS show its own dialog and list this terminal under
    // Privacy & Security > Accessibility; the user still has to switch it on.
    let askOptions = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
    guard AXIsProcessTrustedWithOptions(askOptions) else {
        FileHandle.standardError.write(Data("""
        INCONCLUSIVE: this terminal is not allowed to use the Accessibility API yet.
        macOS should have shown a dialog: choose "Open System Settings", switch this terminal ON under
        Privacy & Security > Accessibility (or add it with the + button), then run again.\n
        """.utf8))
        return 2
    }
    guard let app = NSWorkspace.shared.frontmostApplication, app.bundleIdentifier != "com.apple.loginwindow" else {
        FileHandle.standardError.write(Data("INCONCLUSIVE: cannot see the frontmost application; run this from the terminal window itself.\n".utf8))
        return 2
    }
    let appEl = AXUIElementCreateApplication(app.processIdentifier)
    let focused = attr(appEl, kAXFocusedUIElementAttribute as String).map { $0 as! AXUIElement }
    let window = attr(appEl, kAXFocusedWindowAttribute as String).map { $0 as! AXUIElement }
    let windowFrame: CGRect? = window.flatMap { w in
        guard let p = attr(w, kAXPositionAttribute as String), let s = attr(w, kAXSizeAttribute as String) else { return nil }
        var pt = CGPoint.zero, sz = CGSize.zero
        guard AXValueGetValue(p as! AXValue, .cgPoint, &pt), AXValueGetValue(s as! AXValue, .cgSize, &sz) else { return nil }
        return CGRect(origin: pt, size: sz)
    }

    var report: [String: Any] = [
        "tool": "anchor-probe", "note": note,
        "macos": ProcessInfo.processInfo.operatingSystemVersionString,
        "app": app.localizedName ?? "?", "bundle": app.bundleIdentifier ?? "?",
        "app_version": Bundle(url: app.bundleURL ?? URL(fileURLWithPath: "/"))?.infoDictionary?["CFBundleShortVersionString"] ?? "?",
        "window_frame": rectJSON(windowFrame),
        "screen_count": NSScreen.screens.count,
        "screens": NSScreen.screens.map { s in ["frame": ["x": s.frame.origin.x, "y": s.frame.origin.y, "w": s.frame.width, "h": s.frame.height], "scale": s.backingScaleFactor] },
    ]
    guard let el = focused else {
        report["verdict"] = "NO-GO: no focused UI element exposed"
        return finish(report, out, 1)
    }
    let focusedFrame = axRect(attr(el, "AXFrame"))
    report["focused_frame"] = rectJSON(focusedFrame)
    report["focused_role"] = attr(el, kAXRoleAttribute as String) as? String ?? "?"
    report["focused_subrole"] = attr(el, kAXSubroleAttribute as String) as? String ?? NSNull()
    report["attributes"] = attrNames(el)
    report["parameterized_attributes"] = paramNames(el)
    report["has_selected_text_range"] = attrNames(el).contains("AXSelectedTextRange")
    report["has_bounds_for_range"] = paramNames(el).contains("AXBoundsForRange")
    guard attrNames(el).contains("AXSelectedTextRange"), paramNames(el).contains("AXBoundsForRange") else {
        report["verdict"] = "NO-GO: the focused element does not expose AXSelectedTextRange + AXBoundsForRange"
        return finish(report, out, 1)
    }

    // Cell size and grid from the terminal itself (independent of the Accessibility API).
    let t: [UInt8] = [UInt8(ascii: "t")], osc: [UInt8] = [0x07, UInt8(ascii: "\\")]
    let (cell16, grid, iterm): ([Double]?, [Double]?, [Double]?) = withRawTTY {
        let c = numbers(query("\u{1B}[16t", terminators: t))
        let g = numbers(query("\u{1B}[18t", terminators: t))
        // iTerm2: OSC 1337 ; ReportCellSize  ->  ...ReportCellSize=<h>;<w>;<scale>
        var it: [Double]? = nil
        if let r = query("\u{1B}]1337;ReportCellSize\u{07}", terminators: osc), let eq = r.range(of: "ReportCellSize=") {
            it = r[eq.upperBound...].split(whereSeparator: { ";\u{07}\u{1B}\\".contains($0) }).compactMap { Double($0) }
        }
        return (c, g, it)
    }
    let rows = Int(grid?[1] ?? 24), cols = Int(grid?[2] ?? 80)
    report["terminal_grid"] = ["rows": rows, "cols": cols, "reported_by_terminal": grid != nil]
    report["terminal_cell_px_16t"] = cell16.map { ["h": $0[1], "w": $0[2]] } ?? NSNull()
    report["terminal_cell_iterm_report"] = iterm.map { $0 as Any } ?? NSNull()

    // Fixture corpus. The cursor is only ever placed where a real prompt would put it: inside text,
    // or right after the last typed character. (Blank cells are not representable: the text exposed
    // to the Accessibility API drops trailing blanks, so an index there lands on a later line.)
    struct Sample { let kind: String; let row: Int; let col: Int; let rect: CGRect?; let loc: Int?; let call: String }
    var samples = [Sample]()
    func measure(_ kind: String, _ r: Int, _ c: Int) {
        send("\u{1B}[\(r);\(c)H")
        Thread.sleep(forTimeInterval: 0.15) // let the app publish the new selection
        let range = attr(el, "AXSelectedTextRange").flatMap(axRange)
        var rect: CGRect? = nil, call = "none"
        if let rg = range {
            if let b = bounds(el, rg.location, 0), b.width > 0 || b.height > 0 { rect = b; call = "length0" }
            else if let b = bounds(el, rg.location, 1), b.width > 0 || b.height > 0 { rect = b; call = "length1" }
        }
        samples.append(Sample(kind: kind, row: r, col: c, rect: rect, loc: range?.location, call: call))
    }
    let rs = Array(Set([1, 2, rows / 2, rows - 2, rows - 1, rows].filter { $0 >= 1 && $0 <= rows })).sorted()
    let cs = Array(Set([1, 5, 20, cols / 2, cols - 5].filter { $0 >= 1 && $0 < cols })).sorted()
    let lens = [3, 20, 60].filter { $0 < cols - 1 }

    send("\u{1B}7\u{1B}[?1049h\u{1B}[2J") // save cursor, enter the alternate screen (restored at the end)
    // A: in text. Rows full of text, cursor on characters.
    for r in rs { send("\u{1B}[\(r);1H" + String(repeating: "x", count: cols - 1)) }
    for r in rs { for c in cs { measure("in-text", r, c) } }
    // B: prompt-like. Typed text of a known length, cursor right after it.
    send("\u{1B}[2J")
    for r in rs { for n in lens {
        send("\u{1B}[\(r);1H\u{1B}[2K" + String(repeating: "x", count: n))
        measure("end-of-text", r, n + 1)
    } }
    send("\u{1B}[?1049l\u{1B}8") // back to the normal screen, cursor restored

    // Judge against the cell grid. The grid origin is the median origin implied by the one-cell-tall
    // rectangles (no single sample is trusted as the reference).
    let inText = samples.filter { $0.kind == "in-text" }
    let scales = Set([1.0, Double(NSScreen.main?.backingScaleFactor ?? 2.0)] + NSScreen.screens.map { Double($0.backingScaleFactor) })
    func median(_ v: [Double]) -> Double? { v.isEmpty ? nil : v.sorted()[v.count / 2] }
    let heights = samples.compactMap { $0.rect?.height }.filter { $0 > 0 }
    let oneLine = (heights.min() ?? 0) * 1.5 // anything taller spans two lines
    let single = inText.filter { ($0.rect?.height ?? .infinity) < oneLine }
    var dxs = [Double](), dys = [Double]()
    for a in single { for b in single {
        if a.row == b.row && b.col > a.col { dxs.append(Double(b.rect!.minX - a.rect!.minX) / Double(b.col - a.col)) }
        if a.col == b.col && b.row > a.row { dys.append(Double(b.rect!.minY - a.rect!.minY) / Double(b.row - a.row)) }
    } }
    let axEstimate: (w: Double, h: Double)? = median(dxs).flatMap { w in median(dys).map { (w, $0) } }
    if let ax = axEstimate { report["ax_cell_estimate_pt"] = ["w": ax.w, "h": ax.h] }

    var cellPt: (w: Double, h: Double)?
    var cellSource = "none"
    var scaleUsed = 1.0
    if let cell16 {
        if let ax = axEstimate, let sc = scales.min(by: { abs(cell16[2] / $0 - ax.w) < abs(cell16[2] / $1 - ax.w) }) { scaleUsed = sc }
        cellPt = (cell16[2] / scaleUsed, cell16[1] / scaleUsed)
        cellSource = "terminal-16t"
    } else if let it = iterm, it.count >= 2 {
        cellPt = (it[1], it[0]) // iTerm2 reports points: height;width;scale
        cellSource = "terminal-iterm-ReportCellSize"
    } else if let ax = axEstimate {
        cellPt = ax
        cellSource = "ax-fit (weaker: not independent of the API)"
    }
    var inStrict = 0, inRule = 0, endStrict = 0, endRule = 0, tall = 0
    var boxMatchesPitch = false
    if let cp = cellPt, cp.w > 0, cp.h > 0,
       let x0 = median(single.map { Double($0.rect!.minX) - Double($0.col - 1) * cp.w }),
       let y0 = median(single.map { Double($0.rect!.minY) - Double($0.row - 1) * cp.h }) {
        report["grid_origin_pt"] = ["x": x0, "y": y0]
        report["grid_origin_inside_text_area"] = focusedFrame.map { $0.contains(CGPoint(x: x0, y: y0)) } as Any? ?? NSNull()
        // A caret at the start of a line can come back two lines tall, starting on the line above;
        // "with the tall-rect rule" the caret is the last line of that rectangle.
        func ok(_ s: Sample, rule: Bool) -> Bool {
            guard let r = s.rect else { return false }
            var y = Double(r.minY)
            if rule && Double(r.height) >= 1.5 * cp.h { y = Double(r.maxY) - cp.h }
            let insideWindow = windowFrame.map { $0.insetBy(dx: -2, dy: -2).contains(CGPoint(x: r.midX, y: CGFloat(y) + CGFloat(cp.h) / 2)) } ?? true
            return abs(Double(r.minX) - (x0 + Double(s.col - 1) * cp.w)) <= 0.5 * cp.w
                && abs(y - (y0 + Double(s.row - 1) * cp.h)) <= 0.5 * cp.h && insideWindow
        }
        let ends = samples.filter { $0.kind == "end-of-text" }
        inStrict = inText.filter { ok($0, rule: false) }.count
        inRule = inText.filter { ok($0, rule: true) }.count
        endStrict = ends.filter { ok($0, rule: false) }.count
        endRule = ends.filter { ok($0, rule: true) }.count
        tall = samples.filter { ($0.rect?.height ?? 0) >= 1.5 * cp.h }.count
        report["in_text"] = ["judged": inText.count, "correct_strict": inStrict, "correct_with_tall_rect_rule": inRule]
        report["end_of_text"] = ["judged": ends.count, "correct_strict": endStrict, "correct_with_tall_rect_rule": endRule]
        report["tall_rects"] = tall
        // Evidence that does not need the terminal to report its cell size:
        // (1) the box the API gives for a single character equals the pitch between positions;
        // (2) how the grid sits inside the window (insets) — a grid that fills the window minus equal margins
        //     is hard to get right by accident.
        let boxes = inText.filter { $0.call == "length1" }.compactMap { $0.rect }
        if let bw = median(boxes.map { Double($0.width) }), let bh = median(boxes.map { Double($0.height) }) {
            report["single_char_box_pt"] = ["w": bw, "h": bh]
            boxMatchesPitch = abs(bw - cp.w) <= 0.5 && abs(bh - cp.h) <= 0.5
        }
        report["box_matches_pitch"] = boxMatchesPitch
        if let wf = windowFrame {
            report["grid_insets_in_window_pt"] = ["left": x0 - wf.minX, "top": y0 - wf.minY,
                                                  "right": wf.maxX - (x0 + Double(cols) * cp.w), "bottom": wf.maxY - (y0 + Double(rows) * cp.h)]
        }
    }
    report["cell_size_pt"] = cellPt.map { ["w": $0.w, "h": $0.h] } ?? NSNull()
    report["cell_size_source"] = cellSource
    report["scale_assumed"] = scaleUsed
    report["samples"] = samples.map { ["kind": $0.kind, "row": $0.row, "col": $0.col, "text_index": $0.loc.map { $0 as Any } ?? NSNull(), "bounds_call": $0.call, "rect": rectJSON($0.rect)] }
    report["positions_without_rect"] = samples.filter { $0.rect == nil }.count

    func pct(_ key: String, _ field: String) -> Double? {
        guard let d = report[key] as? [String: Int], let j = d["judged"], j > 0, let c = d[field] else { return nil }
        return 100.0 * Double(c) / Double(j)
    }
    let pIn = pct("in_text", "correct_with_tall_rect_rule"), pEnd = pct("end_of_text", "correct_with_tall_rect_rule")
    let sIn = pct("in_text", "correct_strict"), sEnd = pct("end_of_text", "correct_strict")
    func r1(_ v: Double?) -> Any { v.map { ($0 * 10).rounded() / 10 } ?? NSNull() }
    report["in_text_percent_strict"] = r1(sIn); report["end_of_text_percent_strict"] = r1(sEnd)
    report["in_text_percent_with_tall_rect_rule"] = r1(pIn); report["end_of_text_percent_with_tall_rect_rule"] = r1(pEnd)
    let verdict: String
    if pIn == nil || pEnd == nil { verdict = "NO-GO: no usable caret rectangles (the API did not follow the cursor)" }
    else if min(pIn!, pEnd!) >= 98 && !cellSource.hasPrefix("terminal") && boxMatchesPitch {
        verdict = "PASS (corroborated): the terminal did not report its cell size, but the single-character box equals the position pitch (see grid_insets_in_window_pt); weaker than a terminal-reported size"
    }
    else if min(pIn!, pEnd!) >= 98 && !cellSource.hasPrefix("terminal") { verdict = "INCONCLUSIVE: positions are self-consistent, but the terminal did not report its cell size and the character box does not match the pitch" }
    else if min(pIn!, pEnd!) >= 98 {
        let needsRule = min(sIn!, sEnd!) < 98
        verdict = "PASS for this configuration (>= 98% anchored correctly)" + (needsRule ? " — only with the tall-rect rule (\(tall) samples)" : "")
    }
    else { verdict = "FAIL for this configuration (in-text \(String(format: "%.1f", pIn!))%, end-of-text \(String(format: "%.1f", pEnd!))%; need >= 98%)" }
    report["verdict"] = verdict
    return finish(report, out, verdict.hasPrefix("PASS") ? 0 : 1)
}

func finish(_ report: [String: Any], _ out: String?, _ code: Int32) -> Int32 {
    do {
        let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
        if let out { try data.write(to: URL(fileURLWithPath: out)) } else { FileHandle.standardOutput.write(data) }
    } catch {
        FileHandle.standardError.write(Data("could not write the report: \(error)\n".utf8))
        return 3
    }
    FileHandle.standardError.write(Data("\n\(report["app"] ?? "?"): \(report["verdict"] ?? "?")\(out.map { "  (details: \($0))" } ?? "")\n".utf8))
    return code
}

exit(probe())
