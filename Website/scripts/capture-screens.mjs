import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, "..", "assets", "screens");
const base = process.env.SS_URL || "http://localhost:1420";

const commands = [
  {
    id: "1",
    name: "discord",
    aliases: "dc",
    response: "Join us at discord.gg/example — welcome ${user}!",
    enabled: true,
    permission: "everyone",
    globalCooldown: 5,
    userCooldown: 15,
  },
  {
    id: "2",
    name: "socials",
    aliases: "",
    response: "Twitter/X + YouTube linked in the panels.",
    enabled: true,
    permission: "everyone",
    globalCooldown: 10,
    userCooldown: 30,
  },
  {
    id: "3",
    name: "lurk",
    aliases: "",
    response: "Enjoy the lurk, ${user} 👀",
    enabled: false,
    permission: "everyone",
    globalCooldown: 3,
    userCooldown: 10,
  },
];

const timers = [
  {
    id: "t1",
    name: "Discord",
    message: "Hop in Discord for clips & alerts!",
    intervalMins: 12,
    minChatLines: 8,
    enabled: true,
    liveOnly: true,
  },
  {
    id: "t2",
    name: "Follow",
    message: "Thanks for hanging out — follow for stream pings.",
    intervalMins: 15,
    minChatLines: 5,
    enabled: true,
    liveOnly: false,
  },
];

const giveaways = [
  {
    id: "g1",
    title: "Drop night",
    prize: "Steam key",
    entryCommand: "!enter",
    drawCommand: "!pickwinner",
    durationMins: 10,
    winnerCount: 1,
    eligibility: "everyone",
    excludeMods: true,
    confirmEntry: false,
    announceTemplate:
      "🎉 Congrats ${winner}! You won ${prize}! (${entries} entered)",
    enabled: true,
  },
];

const history = [
  {
    runId: "r1",
    giveawayId: "g1",
    title: "Drop night",
    prize: "Steam key",
    startedAt: new Date().toISOString(),
    endsAt: null,
    entryCount: 84,
    winners: [{ userId: "u1", login: "pixelpine" }],
  },
  {
    runId: "r2",
    giveawayId: "g1",
    title: "Raid reward",
    prize: "Merch code",
    startedAt: new Date(Date.now() - 86400000).toISOString(),
    endsAt: null,
    entryCount: 120,
    winners: [{ userId: "u2", login: "tealwave" }],
  },
];

const handlers = {
  get_status: {
    connected: true,
    connecting: false,
    botLogin: "techjeeper",
    channel: "techjeeper",
    live: true,
    lastError: null,
    chatLines: 248,
    setupComplete: true,
  },
  get_settings: {
    clientId: "demo",
    channel: "techjeeper",
    botLogin: "techjeeper",
    accountMode: "streamer",
    setupComplete: true,
    confirmGiveawayEntry: false,
    timersLiveOnly: false,
    theme: "dark",
  },
  list_commands: commands,
  list_timers: timers,
  list_giveaways: giveaways,
  list_giveaway_history: history,
  get_active_giveaway: null,
  list_automations: [
    {
      id: "a1",
      name: "Sub thanks",
      triggerType: "subscribe",
      actionType: "chat",
      actionPayload: "Thanks for the sub, ${user}!",
      enabled: true,
      cooldownSecs: 0,
    },
  ],
  list_variables: [
    { id: "v1", name: "discord", value: "discord.gg/example" },
    { id: "v2", name: "schedule", value: "Tue/Thu/Sat 8pm ET" },
  ],
  plugin_autostart_is_enabled: false,
};

async function main() {
  await mkdir(outDir, { recursive: true });
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1.25,
  });

  await page.addInitScript((map) => {
    const invoke = async (cmd) => {
      if (cmd in map) return structuredClone(map[cmd]);
      if (cmd.startsWith("plugin:autostart")) return false;
      console.warn("[demo mock] unhandled", cmd);
      return null;
    };
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke,
        transformCallback: (cb, once) => {
          const id = Math.floor(Math.random() * 1e9);
          window[`_${id}`] = once
            ? (payload) => {
                Reflect.deleteProperty(window, `_${id}`);
                return cb(payload);
              }
            : cb;
          return id;
        },
        unregisterCallback: () => {},
      },
    });
    // Event listen no-ops for demo screenshots
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, handlers);

  const shots = [
    ["/", "dashboard.png"],
    ["/commands", "commands.png"],
    ["/timers", "timers.png"],
    ["/giveaways", "giveaways.png"],
  ];

  for (const [route, file] of shots) {
    await page.goto(`${base}${route}`, { waitUntil: "networkidle" });
    await page.waitForTimeout(700);
    // Hide setup if somehow shown
    const title = await page.locator("h1").first().textContent().catch(() => "");
    if (title && /Streamry/i.test(title) && (await page.locator(".setup-card").count())) {
      console.warn(`Route ${route} still on setup — mock may have failed`);
    }
    const target = path.join(outDir, file);
    await page.screenshot({ path: target, type: "png" });
    console.log("wrote", target);
  }

  await browser.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
