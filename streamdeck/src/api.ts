import * as fs from "node:fs";
import * as path from "node:path";

export type Connection = {
  baseUrl: string;
  token: string;
  port: number;
};

const DEFAULT: Connection = {
  baseUrl: "http://127.0.0.1:1920",
  token: "",
  port: 1920,
};

function pluginRoot(): string {
  return path.resolve(__dirname, "..");
}

export function loadConnection(): Connection {
  const file = path.join(pluginRoot(), "streamry-connection.json");
  try {
    if (fs.existsSync(file)) {
      const raw = JSON.parse(fs.readFileSync(file, "utf8")) as Partial<Connection>;
      return {
        baseUrl: raw.baseUrl || `http://127.0.0.1:${raw.port || 1920}`,
        token: raw.token || "",
        port: raw.port || 1920,
      };
    }
  } catch {
    // fall through
  }
  return { ...DEFAULT };
}

export async function apiFetch(
  method: string,
  apiPath: string,
  body?: unknown,
): Promise<Response> {
  const conn = loadConnection();
  if (!conn.token) {
    throw new Error("Not linked to Streamry. Use Install StreamDeck Integration in Settings.");
  }
  const url = `${conn.baseUrl.replace(/\/$/, "")}${apiPath}`;
  const res = await fetch(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      "X-Streamry-Token": conn.token,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    let detail = res.statusText;
    try {
      const j = (await res.json()) as { error?: string };
      if (j.error) detail = j.error;
    } catch {
      // ignore
    }
    throw new Error(detail || `HTTP ${res.status}`);
  }
  return res;
}

export async function apiJson<T>(method: string, apiPath: string, body?: unknown): Promise<T> {
  const res = await apiFetch(method, apiPath, body);
  return (await res.json()) as T;
}
