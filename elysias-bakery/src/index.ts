import { Elysia, status, t } from "elysia";
import { staticPlugin } from "@elysiajs/static";
import { join } from "path";
import { mkdirSync, existsSync } from "fs";
import { $ } from "bun";

const NOTES_DIR = join(import.meta.dir, "..", "notes");
if (!existsSync(NOTES_DIR)) mkdirSync(NOTES_DIR, { recursive: true });

const users = new Map<
  string,
  {
    password: string;
    isAdmin: boolean;
  }
>();

const notes = new Map<string, string[]>();

users.set("admin", {
  password: await Bun.password.hash(Bun.env.ADMIN_PASSWORD || "admin_password"),
  isAdmin: true,
});

function generateNoteId(): string {
  return Bun.randomUUIDv7();
}

function noteFilePath(noteId: string): string {
  return join(NOTES_DIR, `${noteId}.txt`);
}

function getSessionUser(session: any): string | null {
  if (!session.value) return null;
  return typeof session.value === "string" ? session.value : null;
}

function getSessionData(
  session: any,
): { username: string; isAdmin: boolean } | null {
  const username = getSessionUser(session);
  if (!username) return null;
  const user = users.get(username);
  if (!user) return null;
  return { username, isAdmin: user.isAdmin };
}

const app = new Elysia({
  cookie: {
    secrets: [Bun.env.SECRET_KEY || "super_secret_key"],
    sign: ["session"],
  },
})
  .use(staticPlugin({ assets: "public", prefix: "/" }))
  .get("/", () => Bun.file("public/index.html"))

  .post(
    "/signup",
    async ({ body }) => {
      const { username, password } = body;

      if (users.has(username)) {
        return status(400, "Username already exists");
      }
      users.set(username, {
        password: await Bun.password.hash(password),
        isAdmin: false,
      });
      notes.set(username, []);
      return { message: "Signup successful" };
    },
    {
      body: t.Object({
        username: t.String({ minLength: 6 }),
        password: t.String(),
      }),
    },
  )

  .post(
    "/login",
    async ({ body, cookie: { session } }) => {
      const { username, password } = body;

      const user = users.get(username);
      if (!user) {
        return status(401, "Invalid username or password");
      }
      if (!(await Bun.password.verify(password, user.password))) {
        return status(401, "Invalid username or password");
      }
      session.value = username;
      return { message: "Login successful" };
    },
    {
      body: t.Object({
        username: t.String(),
        password: t.String(),
      }),
    },
  )

  .post("/logout", ({ cookie: { session } }) => {
    session.remove();
    return { message: "Logged out" };
  })

  .post(
    "/notes",
    async ({ body, cookie: { session } }) => {
      const username = getSessionUser(session);
      if (!username) return status(401, "Unauthorized");

      const noteId = generateNoteId();
      const filePath = noteFilePath(noteId);

      await Bun.write(filePath, body.content);

      const userNotes = notes.get(username) ?? [];
      userNotes.push(noteId);
      notes.set(username, userNotes);

      return { message: "Note created", noteId };
    },
    {
      body: t.Object({
        content: t.String({ minLength: 1 }),
      }),
    },
  )

  .get("/notes", ({ cookie: { session } }) => {
    const username = getSessionUser(session);
    if (!username) return status(401, "Unauthorized");

    const userNotes = notes.get(username) ?? [];
    return { notes: userNotes };
  })

  .get(
    "/notes/:id",
    async ({ params, cookie: { session } }) => {
      const username = getSessionUser(session);
      if (!username) return status(401, "Unauthorized");

      const userNotes = notes.get(username) ?? [];
      if (!userNotes.includes(params.id)) {
        return status(403, "Forbidden");
      }

      const filePath = noteFilePath(params.id);
      const file = Bun.file(filePath);
      if (!(await file.exists())) {
        return status(404, "Note not found");
      }

      const content = await file.text();
      return { noteId: params.id, content };
    },
    {
      params: t.Object({
        id: t.String(),
      }),
    },
  )
  .delete(
    "/notes/:id",
    async ({ params, cookie: { session } }) => {
      const username = getSessionUser(session);
      if (!username) return status(401, "Unauthorized");

      const userNotes = notes.get(username) ?? [];
      if (!userNotes.includes(params.id)) {
        return status(403, "Forbidden");
      }

      const filePath = noteFilePath(params.id);
      const file = Bun.file(filePath);
      if (await file.exists()) {
        const { unlink } = await import("fs/promises");
        await unlink(filePath);
      }

      notes.set(
        username,
        userNotes.filter((id) => id !== params.id),
      );

      return { message: "Note deleted" };
    },
    {
      params: t.Object({
        id: t.String(),
      }),
    },
  )

  .post("/admin/list", async ({ cookie: { session }, body }) => {
    const data = getSessionData(session);
    if (!data) return status(401, "Unauthorized");
    if (!data.isAdmin) return status(403, "Forbidden");

    const folder = (body as any).folder;

    if (typeof folder === "string" && folder.includes("..")) {
      return status(400, "Invalid folder path");
    }
    try {
      const result = $`ls ${folder}`.quiet();
      const output = await result.text();
      const files = output.split("\n").filter(Boolean);
      return { files };
    } catch (err: any) {
      const stderr = err.stderr?.toString?.() ?? "";
      const stdout = err.stdout?.toString?.() ?? "";
      return { error: stdout + stderr || err.message };
    }
  })
  .listen(3000);

console.log(
  `🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`,
);
