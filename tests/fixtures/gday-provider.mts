import { createServer } from "node:http";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

// This loopback-only fixture uses disposable databases and synthetic credentials.
const repository = process.env.GDAY_REPO_DIR;
if (!repository)
  throw new Error("Set GDAY_REPO_DIR to your GdayMeetings checkout");
const source = (file: string) =>
  pathToFileURL(path.join(repository, "src", file)).href;
const directory = await mkdtemp(path.join(tmpdir(), "gday-real-rust-"));
let handler: (request: Request) => Promise<Response>;
const server = createServer(async (req, res) => {
  try {
    const chunks = [];
    for await (const chunk of req) chunks.push(Buffer.from(chunk));
    const headers = new Headers();
    for (const [key, value] of Object.entries(req.headers))
      if (value)
        headers.set(key, Array.isArray(value) ? value.join(", ") : value);
    const response = await handler(
      new Request(process.env.SERVER_URL + req.url, {
        method: req.method,
        headers,
        body: chunks.length ? Buffer.concat(chunks) : undefined,
      }),
    );
    const out = Object.fromEntries(response.headers);
    const cookies = response.headers.getSetCookie();
    res.writeHead(response.status, {
      ...out,
      ...(cookies.length ? { "set-cookie": cookies } : {}),
    });
    res.end(Buffer.from(await response.arrayBuffer()));
  } catch (error) {
    console.error(error);
    res.writeHead(500);
    res.end("Fixture error");
  }
});
await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = (server.address() as { port: number }).port;
Object.assign(process.env, {
  NODE_ENV: "production",
  SERVER_URL: `http://127.0.0.1:${port}`,
  DATA_DIR: directory,
  DATABASE_ADAPTER: "sqlite",
  DATABASE_URI: `file:${directory}/payload.db`,
  AUTH_DATABASE_URI: `file:${directory}/auth.db`,
  RUNPOD_ENDPOINT_URL: "",
  RUNPOD_API_KEY: "",
  OIDC_UPSTREAM_ISSUER: "",
  OIDC_UPSTREAM_CLIENT_ID: "",
  OIDC_UPSTREAM_CLIENT_SECRET: "",
  PAYLOAD_SECRET: "synthetic-rust-integration-test-secret-32-characters",
});
const { cms } = await import(source("server/payload.ts"));
const { getAuth } = await import(source("server/auth/index.ts"));
const { discovery } = await import(source("server/auth/discovery.ts"));
const platform = await import(
  source("app/(site)/api/platform/[...path]/route.ts")
);
const { POST: upload } = await import(source("app/(site)/upload/route.ts"));
const payload = await cms();
await payload.create({
  collection: "users",
  data: {
    email: "rust-integration@example.test",
    password: "synthetic-test-password-not-production",
    role: "member",
  },
  overrideAccess: true,
});
const auth = await getAuth();
(await auth.$context).rateLimit.enabled = false;
handler = async (request) => {
  const url = new URL(request.url);
  if (url.pathname === "/.well-known/openid-configuration")
    return discovery(request, "oidc");
  if (url.pathname === "/upload") return upload(request);
  if (url.pathname.startsWith("/api/platform/"))
    return platform.GET(request, {
      params: Promise.resolve({
        path: url.pathname.slice("/api/platform/".length).split("/"),
      }),
    });
  return auth.handler(request);
};
if (process.env.GDAY_FIXTURE_READY_FILE)
  await writeFile(process.env.GDAY_FIXTURE_READY_FILE, process.env.SERVER_URL!);
console.log("READY", process.env.SERVER_URL);
async function stop() {
  server.close();
  await payload.destroy();
  await rm(directory, { recursive: true, force: true });
  process.exit(0);
}
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
