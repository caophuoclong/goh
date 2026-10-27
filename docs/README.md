# Terminal Companion — Fig-inspired Rebuild

**Bộ tài liệu nền tảng sản phẩm & kỹ thuật · v0.2 · 27/10/2026**

> v0.2 thêm design system, prototype tương tác (EN/VI), hợp đồng UI/accessibility, schema IPC, hướng dẫn triển khai và cập nhật backlog/QA/ADR/rủi ro tương ứng.

> **Tên dự án: `goh`** (chủ dự án quyết định). Mã nguồn, binary (`goh`, `gohd`, `goh-popup`), biến môi trường (`GOH_*`) và thư mục cấu hình (`~/.config/goh/`) đã được đổi sang tên này; tên `Terminal Companion` trong tài liệu cũ vẫn là tên làm việc. Các chỗ nhắc tới "Fig" (viết hoa) là sản phẩm gốc làm nguồn cảm hứng, không phải tên của chúng ta.
>
> Tên `Terminal Companion` là **working name**, không phải tên thương hiệu chính thức. Đây là dự án độc lập lấy cảm hứng từ trải nghiệm autocomplete của Fig/withfig, **không phụ thuộc Amazon Q/Kiro CLI**. Không sử dụng logo/tên Fig để ngụ ý liên kết hoặc chứng thực.

## Mục tiêu

Một terminal companion **đẹp, nhẹ, riêng tư, cross-platform**: gợi ý subcommand/option/argument theo ngữ cảnh ngay trong terminal người dùng đã chọn. Popup **ưu tiên kế thừa màu terminal hiện tại**; nếu hệ điều hành/terminal không cho phép popup ngoài, cung cấp **inline fallback** thay vì hứa hỗ trợ giả.

## Bắt đầu từ đâu?

1. Đọc [00 — Vision & Assumptions](docs/00-vision.md), sau đó [01 — PRD](docs/01-prd.md).
2. Chốt 5 quyết định trong [13 — Risks & Open Questions](docs/13-risks-open-questions.md).
3. Dùng [03 — Requirements & Acceptance](docs/03-requirements.md) làm phạm vi nghiệm thu.
4. Chạy các proof-of-concept trong [05 — Shell & Terminal Integration](docs/05-terminal-integration.md) **trước khi viết UI lớn**.
5. Phân công từ [14 — Backlog](docs/14-backlog.md), đo tiến độ theo [11 — Roadmap](docs/11-roadmap.md).
6. Bắt tay làm theo [16 — Implementation Guide](docs/16-implementation-guide.md); mở [prototype](prototype/terminal-companion-prototype.vi.html) trong trình duyệt để xem hành vi mục tiêu và đọc [17 — Design System Handoff](docs/17-design-system-handoff.md).

## Danh mục tài liệu

| File | Nội dung | Đối tượng |
|---|---|---|
| [00-vision.md](docs/00-vision.md) | Tầm nhìn, nguyên tắc, giả định, phạm vi sản phẩm | Product/Founder |
| [01-prd.md](docs/01-prd.md) | PRD, nhu cầu, thành công, phạm vi MVP | Product/Engineering |
| [02-users-and-flows.md](docs/02-users-and-flows.md) | Personas, user stories, use cases, flows | Product/Design/QA |
| [03-requirements.md](docs/03-requirements.md) | Functional / non-functional requirements, AC | Engineering/QA |
| [04-architecture.md](docs/04-architecture.md) | Kiến trúc, component, data flow, failure handling | Tech Lead |
| [05-terminal-integration.md](docs/05-terminal-integration.md) | Shell hooks, overlay, Windows/X11/Wayland, limitations | Platform Engineering |
| [06-completion-engine.md](docs/06-completion-engine.md) | Parser, suggestion ranking, spec, generator, IPC | Backend/Runtime |
| [07-ux-design-system.md](docs/07-ux-design-system.md) | UX, keyboard, native popup, theme auto-sync | Product/Design |
| [08-security-privacy.md](docs/08-security-privacy.md) | Threat model, sandbox, privacy, permissions | Security/Engineering |
| [09-qa-test-plan.md](docs/09-qa-test-plan.md) | Test cases, matrix, performance benchmarks | QA/Engineering |
| [10-devops-release.md](docs/10-devops-release.md) | Repo layout, CI/CD, installer, telemetry | Engineering/DevOps |
| [11-roadmap.md](docs/11-roadmap.md) | Milestones & release gates | PM/Team |
| [12-adr.md](docs/12-adr.md) | Architectural decision records | Engineering |
| [13-risks-open-questions.md](docs/13-risks-open-questions.md) | Giả định, trade-offs, top risks | Entire team |
| [14-backlog.md](docs/14-backlog.md) | Epics, priority, dependency, definition of done | Engineering |
| [15-sources.md](docs/15-sources.md) | Link tham chiếu & giới hạn nguồn | Entire team |
| [16-implementation-guide.md](docs/16-implementation-guide.md) | Thứ tự triển khai, workspace, engine, daemon, adapter zsh, definition of done | Engineering |
| [17-design-system-handoff.md](docs/17-design-system-handoff.md) | Design system, prototype, ánh xạ sang code, giới hạn đã biết | Product/Design/Engineering |
| [18-spike-results.md](docs/18-spike-results.md) | Kết quả spike bước 8: đã chạy gì, chưa chạy gì, quyết định go/no-go | Engineering/Platform |

### Contract & example

- [`schemas/completion.schema.json`](schemas/completion.schema.json): schema v1 cho completion spec tĩnh.
- [`schemas/config.schema.json`](schemas/config.schema.json): schema v1 của cấu hình người dùng.
- [`schemas/ipc.schema.json`](schemas/ipc.schema.json): schema đề xuất cho các frame IPC (`complete`, `results`, `accept`, `edit`, `stale`).
- [`examples/git.completion.json`](examples/git.completion.json): ví dụ completion tĩnh rút gọn, không đại diện đủ tính năng Git.
- [`examples/config.json`](examples/config.json): ví dụ theme `auto` và giới hạn privacy.
- [`examples/completion-response.json`](examples/completion-response.json): ví dụ phản hồi `results` cho `git ch`, có `match_ranges`.

### Design & prototype

- [`design-system/`](design-system/) (cũng có bản nén [`terminal-companion-design-system.zip`](terminal-companion-design-system.zip)): tokens, README, quy tắc resolve theme, 5 component tham chiếu.
- [`prototype/`](prototype/): prototype tương tác một file, bản [English](prototype/terminal-companion-prototype.en.html) và [Tiếng Việt](prototype/terminal-companion-prototype.vi.html).

## Chính sách đọc tài liệu

- **MUST** = bắt buộc nghiệm thu; **SHOULD** = ưu tiên; **MAY** = tuỳ chọn.
- **P0** = MVP; **P1** = bản ổn định tiếp theo; **P2** = mở rộng.
- Một số lựa chọn công nghệ là **đề xuất/ADR chưa chốt**; các mốc và mục tiêu hiệu năng là **target**, không phải số đo thực tế.
- `Cross-platform` nghĩa là có **một lộ trình/mức hỗ trợ được công bố** trên macOS, Linux, Windows — không có nghĩa popup tuyệt đối giống nhau trên tất cả terminal/compositor.
- Mọi thay đổi ảnh hưởng UX, IPC, schema và bảo mật phải cập nhật docs cùng pull request.

## Definition of MVP (ngắn gọn)

- macOS (zsh), Linux (bash), Windows (PowerShell 7 + PSReadLine): gợi ý với `git`, `npm`, `docker` cùng file/dir, gắn vào shell có kiểm soát và gỡ sạch được.
- Đọc prompt/context theo khả năng shell; hỗ trợ arrow/tab/esc, **không chạy command khi chỉ chọn gợi ý**.
- Theme `auto` dùng màu terminal nếu có nguồn đáng tin; cho phép override và đảm bảo contrast tối thiểu.
- Native overlay cho các environment đã kiểm chứng; inline fallback cho environment bị hạn chế.
- Local-only mặc định, không đăng nhập, không bắt buộc network, không gửi lịch sử lệnh.

## Cần quyết định trước khi bắt đầu implementation

Working name/brand; danh sách terminal bắt buộc ở phiên bản đầu; tính năng remote SSH; chính sách dynamic generator; license repo và tái sử dụng completion specs bên thứ ba. Mỗi quyết định có đề xuất mặc định trong [13](docs/13-risks-open-questions.md) để chỉ cần đồng ý hoặc đổi.

## Trạng thái hiện tại

- **Đã có:** tài liệu sản phẩm/kỹ thuật, schema, design system, prototype; mã nguồn Rust (`crates/`: engine, daemon + IPC, theme/OSC), adapter zsh (menu inline, ghost text, theme) và bash (programmable completion) trong `adapters/`, kiểm thử pty tự động.
- **Chưa có:** adapter PowerShell, adapter zsh đã điều khiển overlay qua `goh-popup` (anchor provider `goh-anchor` + gate đã có; iTerm2 đã chạy thật end-to-end (có ảnh chụp: overlay hiện dưới caret, ↓ đổi mục), Terminal.app chưa; probe S-04 đã PASS cho iTerm2 (1 pane) và Terminal.app (split, 1 pane, màn hình thứ hai); chưa thử dấu nhắc thật, màn hình scale 1; overlay chưa được bật; `goh-popup` mới vẽ + kiểm thử model), installer, số đo hiệu năng thật, hỗ trợ Windows (IPC mới chỉ có Unix socket).
- **Chưa kiểm chứng:** focus của overlay trên Windows/Linux và việc phím vẫn tới terminal (macOS: đã PASS kiểm tra frontmost app), bash 4.x và terminal Linux thật (bash 5.2 trong Docker đã qua), macOS Terminal + iTerm2, các spike anchor/accessibility — xem [18](docs/18-spike-results.md); hành vi screen reader; giả định A-01…A-05.
