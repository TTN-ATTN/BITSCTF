import express from "express";
import { readFileSync } from "fs";
import { join } from "path";
import { v4 as uuidv4 } from "uuid";
import DOMPurify from "isomorphic-dompurify";
import { visit } from "./bot";

const app = express();
const PORT = parseInt(process.env.APP_PORT || "3000");
const APP_HOST = process.env.APP_HOST || "localhost";
const ADMIN_SECRET = process.env.ADMIN_SECRET || "fake_admin_secret";

const pastes = new Map<string, string>();
const indexTemplate = readFileSync(
  join(__dirname, "views", "index.html"),
  "utf-8",
);
const pasteTemplate = readFileSync(
  join(__dirname, "views", "paste.html"),
  "utf-8",
);

app.use(express.urlencoded({ extended: false }));
app.use((req, res, next) => {
  res.setHeader(
    "Content-Security-Policy",
    "script-src 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; default-src 'self'",
  );
  next();
});

app.get("/", (req, res) => {
  res.type("html").send(indexTemplate);
});

app.post("/create", (req, res) => {
  const content = req.body.content;
  if (!content || typeof content !== "string") {
    return res.status(400).send("Content is required");
  }

  const id = uuidv4();
  const clean = DOMPurify.sanitize(content);
  pastes.set(id, clean);

  res.redirect(`/paste/${id}`);
});

app.get("/paste/:id", (req, res) => {
  const content = pastes.get(req.params.id);
  if (!content) {
    return res.status(404).send("Paste not found");
  }

  const html = pasteTemplate.replace("{paste}", content); // mutation XSS using replace to bypass DOMPurify
  // payload: '<img src="abc$`<img src=x onerror=alert(1)>">'
  res.type("html").send(html); // inject payload before sending to browser
});

/* <!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>SafePaste - View Paste</title>
</head>
<body>
  <nav><a href="/">🔒 SafePaste</a></nav>
  <div class="paste-container">
    <img src="/logo.png" alt="SafePaste">
    <div class="content">{paste}</div>
  </div>
</body>
</html>
*/

app.post("/report", async (req, res) => {
  const url = req.body.url;
  if (!url || typeof url !== "string") {
    return res.status(400).send("URL is required");
  }

  try {
    const parsed = new URL(url);
    if (parsed.hostname !== APP_HOST && parsed.hostname !== "localhost") {
      return res.status(400).send("URL must be on this server");
    }
  } catch {
    return res.status(400).send("Invalid URL");
  }

  res.send("Admin will review your paste shortly...");

  visit(url).catch((e) => console.error("Visit failed:", e));
});

app.get("/hidden", (req, res) => {
  if (req.query.secret === ADMIN_SECRET) {
    return res.send("Welcome, admin!");
  }
  res.socket?.destroy();
});

app.listen(PORT, () => {
  console.log(`SafePaste running on http://localhost:${PORT}`);
});
