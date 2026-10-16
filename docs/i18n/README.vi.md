# Trellis

**Không gian làm việc chung để các tác tử AI phối hợp nhiệm vụ, hợp nhất thay đổi và cùng sửa lỗi.**

[English](../../README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [한국어](README.ko.md) · [Português](README.pt.md) · **Tiếng Việt**

Trellis là backend viết bằng Rust dành cho nhiều tác tử AI cùng làm việc trên mã nguồn, tài liệu hoặc dữ liệu có cấu trúc. Mỗi tác tử làm việc trong một phiên riêng; máy chủ chung lưu lịch sử, tích hợp thay đổi vào `main` và thông báo khi nội dung mà tác tử đang phụ thuộc vào bị thay đổi.

Bạn có thể sử dụng qua **CLI**, **MCP**, **HTTP API** hoặc **các crate Rust**. Bạn cung cấp tác tử và môi trường chạy; Trellis cung cấp trạng thái dùng chung và cơ chế phối hợp.

> **Trạng thái: giai đoạn phát triển ban đầu.** Đã triển khai lưu trữ cốt lõi, hợp nhất, điều phối, CLI, máy chủ HTTP và MCP, cầu nối Git và kiểm tra sau khi tích hợp. Đã có kết quả đo hiệu năng ban đầu; API và quy trình làm việc vẫn có thể thay đổi.

## Các khả năng chính

| Khả năng | Lợi ích |
| --- | --- |
| Lịch sử chung | Lưu các tập thay đổi có phiên bản, gắn với tác tử, phiên làm việc và nhiệm vụ. |
| Hợp nhất và xử lý xung đột | Hợp nhất văn bản và thay đổi có cấu trúc trong JSON/YAML; giữ lại xung đột chưa giải quyết để tác tử sửa. |
| Điều phối nhiệm vụ | Nhận nhiệm vụ theo thao tác nguyên tử, khai báo thay đổi dự kiến và dùng quyền truy cập độc quyền có thời hạn khi cần. |
| Ngữ cảnh chung | Ghi lại sự kiện thực tế, lỗi, tóm tắt và quyết định; đọc các mục mới và nhận thông báo khi nội dung đã đọc bị ảnh hưởng. |
| Kiểm tra | Chạy các bước kiểm tra đã cấu hình sau khi thay đổi vào `main`, rồi tạo nhiệm vụ sửa lỗi từ kết quả thất bại. |
| Tương tác với Git | Nhập ảnh chụp trạng thái Git và xuất các tập thay đổi đã tích hợp thành commit Git. |

Quy trình thường dùng là **nhận nhiệm vụ → chỉnh sửa → tích hợp → xử lý phản hồi**. Ở đây, *land* nghĩa là yêu cầu máy chủ tích hợp thay đổi của một phiên vào `main`. Kiểm tra diễn ra sau đó, nên tích hợp thành công không đồng nghĩa với việc các bài kiểm thử đã đạt.

## Bắt đầu nhanh

Cần có **Rust 1.85+**, **Cargo** và **Git**. Các lệnh dưới đây dùng shell tương thích với Bash.

### 1. Cài đặt

```bash
git clone https://github.com/askuy/multi-agent-collaborate.git
cd multi-agent-collaborate
cargo install --locked --path crates/trellis-cli
```

### 2. Khởi động máy chủ chung

Chạy trong một cửa sổ terminal:

```bash
tl serve
```

Địa chỉ mặc định là `http://127.0.0.1:7420`. Dữ liệu được lưu trong `.trellis-data` ở thư mục làm việc của máy chủ. Hãy giữ terminal này mở. Mọi tác tử dùng chung một kho phải kết nối đến cùng một tiến trình máy chủ, vì các khóa điều phối được lưu trong tiến trình đó.

### 3. Chia sẻ thay đổi giữa hai tác tử

Trong terminal khác, dùng một thư mục mới:

```bash
mkdir trellis-demo
cd trellis-demo
export TRELLIS_REPO=demo
tl init

tl --as alice open ./alice
echo "Hello from Alice" > ./alice/notes.md
tl --as alice land ./alice -m "Add a shared note"

tl --as bob open ./bob
cat ./bob/notes.md
```

Bản sao làm việc của Bob giờ chứa `Hello from Alice`. Để tiếp tục sau khi tích hợp, mở phiên tiếp theo bằng `tl --as alice next ./alice`. Kết quả lệnh có định dạng JSON; mã thoát của `land` là `2` khi có xung đột và `3` khi thay đổi bị từ chối.

Xem [hướng dẫn sử dụng](../usage.md) bằng tiếng Anh để tìm hiểu hàng đợi nhiệm vụ, cập nhật, giải quyết xung đột, chính sách và bộ kiểm tra.

## Kết nối tác tử

| Giao diện | Điểm truy cập | Mục đích thường gặp |
| --- | --- | --- |
| CLI | `tl` | Tác tử có quyền dùng shell, thao tác thủ công và script |
| MCP qua stdio | `tl mcp` | Tác tử hỗ trợ MCP |
| HTTP/JSON + SSE | `tl serve` | Ứng dụng viết bằng bất kỳ ngôn ngữ nào |
| Crate Rust | `crates/` | Tích hợp trong tiến trình và ứng dụng khách Rust |

Với Claude Code, khởi động `tl serve` và khởi tạo kho `demo`, rồi thêm cấu hình sau vào `.mcp.json` của dự án:

```json
{
  "mcpServers": {
    "trellis": {
      "command": "tl",
      "args": ["mcp"],
      "env": {
        "TRELLIS_REPO": "demo"
      }
    }
  }
}
```

Đặt định danh riêng cho từng tác tử, chẳng hạn `TRELLIS_PRINCIPAL=agent-1 claude`. Mỗi tác tử chạy một tiến trình `tl mcp` riêng và kết nối đến máy chủ HTTP chung. Đặt `TRELLIS_SERVER` nếu máy chủ dùng địa chỉ khác. Các công cụ MCP hỗ trợ phiên làm việc, đọc và ghi, tích hợp, nhiệm vụ, xung đột, thông báo và ngữ cảnh chung.

## Kết quả đo hiệu năng cho thấy điều gì

[Báo cáo so sánh ngày 16 tháng 10 năm 2026](../dogfood/2026-10-16-final-comparison.md) chạy mỗi cấu hình ba lần với cùng mô hình, mức nỗ lực suy luận và bộ công cụ của tác tử. Đây là các thử nghiệm nhỏ, không phải cam kết về hiệu năng trong mọi trường hợp.

Trung vị thời gian đến khi tất cả bài kiểm thử đều đạt, bao gồm thời gian sửa lỗi cần thiết:

| Cấu hình | 60 nhiệm vụ | Thay đổi giao diện |
| --- | --- | --- |
| Một tác tử | 8:52 | 1:10 |
| Git worktrees, hợp nhất mặc định | 3:52 | 3:21 |
| Git worktrees, hợp nhất union cho nhật ký thay đổi | 3:14 | 1:51 |
| Trellis, không dùng khóa | 3:23 | 2:26 |

Thời gian có dạng phút:giây. Các cấu hình song song dùng năm tác tử cho 60 nhiệm vụ và ba tác tử cho thay đổi giao diện. Trellis đưa công việc đã hoàn thành lên `main` ngay trong quá trình chạy. Với các nhiệm vụ độc lập, Git worktrees dùng hợp nhất union cho nhật ký thay đổi nhanh hơn và ít tốn kém hơn; một tác tử hoàn thành nhanh nhất ở bài thay đổi giao diện nhỏ. Báo cáo còn có chi phí, diễn biến sửa lỗi và kết quả Trellis khi dùng khóa.

## Tài liệu

- [Hướng dẫn sử dụng](../usage.md) — quy trình hằng ngày, cấu hình, cầu nối Git và kiểm tra.
- [Mục lục tài liệu](../README.md) — thiết kế, nghiên cứu và báo cáo thử nghiệm.
- [Đặc tả thiết kế](../superpowers/specs/2026-10-15-trellis-design.md) — kiến trúc, mô hình dữ liệu và kế hoạch.
- [Bộ công cụ đo hiệu năng](../../dogfood/) — kịch bản và script dùng trong các phép so sánh.
- [Nguồn tham khảo](../../references/README.md) — tài liệu về quản lý phiên bản và phối hợp nhiều tác tử.

## Đóng góp

Hoan nghênh issue, báo lỗi, thử nghiệm có thể tái lập và pull request. Đọc đặc tả thiết kế để hiểu kiến trúc, rồi chạy các bước kiểm tra sau khi sửa mã:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Khi cập nhật README, hãy giữ nội dung của cả chín phiên bản ngôn ngữ nhất quán. Tài liệu kỹ thuật bổ sung hiện chủ yếu bằng tiếng Anh.

## Giấy phép

Giấy phép chưa được chọn. Kho hiện chưa có tệp `LICENSE`.
