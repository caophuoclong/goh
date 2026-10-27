# 01 — Product Requirements Document (PRD)

**Product:** Terminal Companion (working name) · **Version:** 0.1 · **Status:** Draft

## Problem statement

Khi gõ `git`, `docker`, `npm`, người dùng cần biết lệnh con, flag, positional arguments và khả năng hoàn thành path. Tab-completion thông thường có thể hỗ trợ nhưng thường thiếu mô tả, bị phụ thuộc shell, và không đồng bộ UX. Fig từng giải bài toán qua completion specs và gợi ý hiển thị ngay tại vị trí gõ. Dự án này tái hiện **giá trị sử dụng**, không sao chép branding hay lệ thuộc Amazon Q CLI.

## Goals và measures

| Mục tiêu | Chỉ số nghiệm thu / target | Ghi chú |
|---|---|---|
| Nhập lệnh không bị phá | 0 lỗi mất/kép phím trong bộ regression | Block release |
| Gợi ý nhanh | Suggestion P95 ≤ 50 ms từ event nhận tại engine đến danh sách kết quả (static cache), P99 ≤ 120 ms | Target cần đo trên hardware mẫu |
| Chèn đúng | ≥ 99% trên bộ golden command-line test | Covers quotes/space/unicode |
| Khởi động nhẹ | Overhead khởi tạo shell hook P95 ≤ 15 ms trên shell test chuẩn | Không bao gồm startup shell của user |
| Tài nguyên gọn | Resident background target ≤ 60 MB, idle CPU < 1% trên máy benchmark; overlay có thể vượt tạm thời | Budget đề xuất, chưa đo |
| Theme phù hợp | 100% terminal có màu được xác định: popup lấy màu nền/chữ, hoặc fallback rõ ràng | Không khẳng định detection universal |
| Riêng tư | 0 command text gửi ra mạng theo mặc định | Verified network test |

## Personas

- **Developer thường xuyên CLI:** chủ yếu git/npm/docker, cần gợi ý không ngắt flow.
- **SRE/Platform engineer:** command nhiều option, quan tâm độ tin cậy, security, SSH/tmux.
- **Người mới học terminal:** cần description và argument hints, lỗi nhập không gây chạy lệnh nguy hiểm.

## Core jobs to be done

- Khi gõ `git ch`, hiển thị `checkout`, `cherry-pick` với mô tả, nhận một lựa chọn bằng Tab.
- Khi gõ `git checkout `, nếu bật generator an toàn, đề xuất branch; nếu chưa bật vẫn hiển thị hints tĩnh.
- Khi gõ `cd ~/Doc`, gợi ý filesystem path có thư mục đúng quyền truy cập.
- Khi chuyển terminal theme từ dark sang light, popup cập nhật tương ứng hoặc báo dùng palette fallback.
- Khi môi trường không hỗ trợ overlay, vẫn sử dụng menu inline hoặc để native completion hoạt động bình thường.

## Feature scope

| Capability | MVP P0 | P1 | P2 |
|---|---|---|---|
| Static specs (JSON) | Có | đầy đủ rộng hơn | registry |
| Shell zsh/bash/pwsh7 | zsh mac, bash Linux, pwsh Windows | thêm tổ hợp OS/shell | tương thích sâu |
| Native overlay | verified subset | mở rộng | advanced positioning |
| Inline fallback | Có | cải thiện layout | — |
| Auto theme | fg/bg, fallback | palette + font best-effort | app-specific adapters |
| File/dir completions | Có | fuzzy matching | remote path |
| Dynamic generator | chỉ built-in allowlisted với opt-in | sandbox policy | community generators |
| Fish shell | Spike | Có | — |
| SSH/tmux | documented limitations | best effort | remote bridge |
| AI command assistant | Không | Không | cân nhắc độc lập |

## Support matrix đề xuất cho MVP

| Platform | Shell chính thức | Baseline terminal | Display mode |
|---|---|---|---|
| macOS (2 phiên bản mới đang được duy trì) | zsh | Terminal.app, iTerm2 | overlay nếu spike đạt; inline dự phòng |
| Linux (Ubuntu LTS X11 + Wayland) | bash | GNOME Terminal, Kitty | overlay X11 best-effort; Wayland inline bắt buộc |
| Windows 11 | PowerShell 7 + PSReadLine | Windows Terminal | overlay nếu UIA đủ dữ liệu; inline dự phòng |

**Không đồng nghĩa mọi terminal ở trên được cam kết overlay ngay.** Nếu spike không đạt, phát hành beta với inline fallback và công bố limitation; không cố ép accessibility/hack compositor.

## MVP user experience

1. Installer/CLI thiết lập adapter với thông báo tác động lên shell rc/profile.
2. Bắt đầu session: adapter xác định shell/session/cwd, gửi event **chỉ khi người dùng nhập ở editable prompt**.
3. Engine parse command prefix trước cursor và trả danh sách gợi ý.
4. UI render tại caret nếu được, nếu không chuyển inline menu; phím điều hướng không làm thay đổi buffer trước khi accept.
5. Accept -> gửi edit thay thế một range ký tự an toàn; shell giữ quyền nhận và sửa buffer.
6. Esc đóng, Enter để shell xử lý bình thường trừ khi cấu hình rõ ràng; tắt/uninstall trả về shell native.

## Launch criteria

- Tất cả AC P0 pass trên matrix chính thức; zero regression nghiêm trọng mất input; không yêu cầu network; clean uninstall; docs và screen-reader/keyboard audit cơ bản.
- 20–30 phiên dùng thử internal có logs **đã lọc nhạy cảm** hoặc feedback do người dùng cung cấp; không sử dụng command nội dung thật như telemetry.
- Release notes ghi rõ terminal/shell/display mode chính thức và unsupported combos.

## Dependencies, exclusions và assumptions

Tham khảo docs 04–06. Rủi ro lớn nhất là **event capture trong Bash/PowerShell** và **global caret location trên Wayland**; là các spike có kill criteria, không gộp trực tiếp vào cả quá trình xây app. Không tích hợp server/backend cho MVP.
