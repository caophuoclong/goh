# 00 — Vision, Principles & Assumptions

**Trạng thái:** Draft v0.1 · **Owner:** Product/Tech Lead · **Ngày:** 2026-10-27

## Product vision

> Mang trải nghiệm autocomplete thông minh kiểu IDE vào **terminal hiện tại** mà không bắt người dùng đổi terminal, đổi cách gõ, đăng nhập một dịch vụ hay hy sinh hiệu năng.

**Đối tượng:** developer, DevOps/SRE, người học CLI. **Vấn đề:** command có nhiều flag/subcommand, người dùng liên tục tra docs; shell completion truyền thống thiếu description và nhất quán giao diện. **Lợi ích:** khám phá lệnh dễ hơn, bớt lỗi nhập command, giữ nguyên thói quen shell.

## Product principles

1. **Terminal-first:** terminal giữ quyền điều khiển input; companion không tự chiếm focus hay phá keyboard shortcut.
2. **Theme-native:** kế thừa foreground/background và font/spacing nếu có API đáng tin; không áp một dark theme riêng.
3. **Lightweight by design:** pipeline tại máy, lazy load specs, không cần Chromium/remote inference cho luồng cơ bản.
4. **Graceful degradation:** Wayland, tmux, remote, IDE tích hợp có thể thiếu caret API. Dùng inline fallback hoặc tắt an toàn.
5. **Least privilege & privacy:** không lưu nội dung command line/historical token; permission phải có lý do rõ.
6. **Open ecosystem:** spec định nghĩa bằng schema có version, local overrides, importer; không ràng buộc vendor.
7. **Predictable editing:** chỉ chèn text người dùng chọn và preserve quoting/cursor, không execute implicit.

## In scope

- Suggestion theo context: executable, subcommands, options/flags, args tĩnh, paths, git branches (dynamic nếu bật).
- Popup keyboard-driven, mô tả và loại suggestion, filter/rank, theme terminal.
- Shell adapters: zsh, bash, PowerShell 7 ở MVP; fish ở phase kế tiếp.
- Runtime/CLI diagnostics `doctor`, quản lý enabled/disabled per shell/terminal.
- Cross-platform releases và trải nghiệm hoạt động offline.

## Out of scope cho MVP

- AI chat/agent tự tạo lệnh; sync tài khoản/team; cloud spec registry; terminal emulator riêng; command-history search trên cloud; remote SSH deep integration; Warp-like blocks; nhập password hoặc can thiệp full-screen TUI.

## Assumptions (cần kiểm chứng)

| ID | Giả định | Cách chứng minh |
|---|---|---|
| A-01 | Native overlay có thể được neo tương đối ổn định trên tập terminal macOS đã chọn | Spike caret-position + focus |
| A-02 | Windows PowerShell 7 / PSReadLine cho phép hook không làm hỏng Tab/history | Spike key handling |
| A-03 | Linux Wayland có môi trường không thể biết vị trí caret toàn hệ thống | Fallback inline trên test matrix |
| A-04 | OSC 10/11 trả màu trong *một phần* terminal, không phổ quát | Probe có timeout + fallback |
| A-05 | Common core Rust + UI nhẹ đạt target tài nguyên | Benchmark thật trên hardware tiêu biểu |

## North-star metric & guardrail

- **North-star:** % phiên dùng terminal được hỗ trợ có ít nhất một suggestion được chấp nhận (opt-in analytics hoặc test nghiên cứu, không gửi command text).
- **Guardrails:** shell startup regression, input latency, crash frequency, lost keystrokes **= 0 trong test gate**, permission friction, độ chính xác completion, accessibility.

## Non-goals kỹ thuật

- Không hook bàn phím toàn hệ thống và không đọc các app khác chỉ để lấy command.
- Không hứa hỗ trợ mọi terminal bằng một implementation duy nhất.
- Không chạy script từ completion specs lấy từ Internet một cách mặc định.
