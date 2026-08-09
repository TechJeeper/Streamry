#!/usr/bin/env node
/**
 * Post Streamry release notes to the Discord releases webhook.
 *
 * Env:
 *   DISCORD_RELEASE_WEBHOOK_URL — full Discord webhook URL (required)
 *
 * Reads Website/version.json:
 *   version, downloadUrl, notes, optional changelog[] (markdown bullets)
 *
 * Usage:
 *   node scripts/announce-discord-release.mjs
 *   node scripts/announce-discord-release.mjs --dry-run
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");
const dryRun = process.argv.includes("--dry-run");

const webhookUrl = (process.env.DISCORD_RELEASE_WEBHOOK_URL || "").trim();
if (!webhookUrl && !dryRun) {
  console.error(
    "Missing DISCORD_RELEASE_WEBHOOK_URL (full Discord webhook URL).",
  );
  process.exit(1);
}

const info = JSON.parse(
  readFileSync(join(root, "Website", "version.json"), "utf8").replace(
    /^\uFEFF/,
    "",
  ),
);
const version = String(info.version || "").trim();
if (!version) {
  console.error("Website/version.json is missing version.");
  process.exit(1);
}

const base =
  "https://techjeeper.github.io/Streamry";
const downloadsPage =
  info.downloadUrl || `${base}/downloads.html`;
const winUrl = `${base}/downloads/Streamry_${version}_x64-setup.exe`;
const macUrl = `${base}/downloads/Streamry_${version}_universal.dmg`;

const changelog = Array.isArray(info.changelog)
  ? info.changelog.map((line) => String(line).trim()).filter(Boolean)
  : [];

const description =
  String(info.notes || "").trim() ||
  `Streamry ${version} is out.`;

const fields = [];
if (changelog.length) {
  fields.push({
    name: "What's new",
    value: changelog.map((line) => `• ${line}`).join("\n").slice(0, 1024),
  });
}
fields.push({
  name: "Download",
  value: `[Windows](${winUrl}) · [macOS](${macUrl}) · [All downloads](${downloadsPage})`,
});

const body = {
  username: "Streamry",
  embeds: [
    {
      title: `Streamry ${version}`,
      url: downloadsPage,
      description,
      color: 0x2ecc71,
      fields,
      footer: {
        text: "After install: reconnect the bot if prompted",
      },
      timestamp: new Date().toISOString(),
    },
  ],
};

if (dryRun) {
  console.log(JSON.stringify(body, null, 2));
  process.exit(0);
}

const res = await fetch(webhookUrl, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});

if (!res.ok) {
  const text = await res.text().catch(() => "");
  console.error(`Discord webhook failed (${res.status}): ${text}`);
  process.exit(1);
}

console.log(`Announced Streamry ${version} to Discord.`);
