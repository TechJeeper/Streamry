import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { AppSettings, DeviceCode } from "../types";

const SCOPES = [
  "chat:read",
  "chat:edit",
  "user:read:chat",
  "channel:read:subscriptions",
  "moderator:read:followers",
];

const TWITCH_CONSOLE = "https://dev.twitch.tv/console";
const TWITCH_CONSOLE_HOME = "https://dev.twitch.tv/console/apps";
const TWITCH_REGISTER = "https://dev.twitch.tv/console/apps/create";
const GUIDE_LAST_STEP = 3;

type Phase =
  | "welcome"
  | "account"
  | "guide"
  | "paste"
  | "bot-create"
  | "bot-mod"
  | "bot-channel"
  | "authorize"
  | "done";

/** How the user entered setup after the welcome screen. */
type SetupPath = "walkthrough" | "paste";

export function Setup({ onDone }: { onDone: () => void }) {
  const [phase, setPhase] = useState<Phase>("welcome");
  const [setupPath, setSetupPath] = useState<SetupPath>("walkthrough");
  const [guideStep, setGuideStep] = useState(0);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [botLoginHint, setBotLoginHint] = useState("");

  useEffect(() => {
    api.getSettings().then((s) => {
      setSettings(s);
      if (s.clientId) {
        setSetupPath("paste");
        setPhase("paste");
      }
    });
  }, []);

  useEffect(() => {
    const unsubs = [
      listen("auth-success", async () => {
        const s = await api.getSettings();
        setSettings(s);
        setDevice(null);
        setPhase("done");
      }),
      listen<string>("auth-error", (e) => setError(String(e.payload))),
    ];
    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  if (!settings) return null;

  async function savePartial(patch: Partial<AppSettings>) {
    const next = { ...settings!, ...patch };
    setSettings(next);
    await api.saveSettings(next);
  }

  async function startLogin() {
    setBusy(true);
    setError("");
    try {
      const d = await api.startDeviceLogin(SCOPES);
      setDevice(d);
      setPhase("authorize");
      await openUrl(d.verificationUri);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    await api.completeSetup();
    try {
      await api.connectBot();
    } catch {
      /* optional */
    }
    onDone();
  }

  function afterClientId() {
    if (settings!.accountMode === "bot") {
      setPhase("bot-mod");
      return;
    }
    void startLogin();
  }

  function afterAccountChoice() {
    if (settings!.accountMode === "bot") {
      setPhase("bot-create");
      return;
    }
    if (setupPath === "walkthrough") {
      setGuideStep(0);
      setPhase("guide");
      return;
    }
    void startLogin();
  }

  function afterBotCreate() {
    if (setupPath === "walkthrough") {
      setGuideStep(0);
      setPhase("guide");
      return;
    }
    setPhase("bot-mod");
  }

  return (
    <div className="setup">
      <div className="setup-card">
        <img src="/sentinel.svg" width={56} height={56} alt="" />
        <h1>Streamry</h1>

        {phase === "welcome" && (
          <>
            <p className="lead">
              Let’s get Streamry talking to Twitch. We’ll help you create a
              free <strong>Client ID</strong> and connect your account — about
              two minutes, one time only.
            </p>
            <div className="choice-grid">
              <button
                className="choice"
                onClick={() => {
                  setSetupPath("walkthrough");
                  setPhase("account");
                }}
              >
                <strong>Walk me through getting started</strong>
                <span>
                  Choose bot or your own account, then we’ll guide you step by
                  step.
                </span>
              </button>
              <button
                className="choice"
                onClick={() => {
                  setSetupPath("paste");
                  setPhase("paste");
                }}
              >
                <strong>I have a Client ID</strong>
                <span>Paste it and continue.</span>
              </button>
            </div>
          </>
        )}

        {phase === "account" && (
          <>
            <p className="lead" style={{ marginBottom: 12 }}>
              Bot or your own account?
            </p>
            <p className="hint" style={{ marginBottom: 16 }}>
              Who should appear in chat when Streamry replies?
            </p>
            <div className="choice-grid">
              <button
                className={`choice ${settings.accountMode === "bot" ? "selected" : ""}`}
                onClick={() => savePartial({ accountMode: "bot" })}
              >
                <strong>A separate bot account</strong>
                <span>
                  Replies show under a bot name. We’ll walk you through creating
                  a second Twitch account if you need one.
                </span>
              </button>
              <button
                className={`choice ${settings.accountMode === "streamer" ? "selected" : ""}`}
                onClick={() => savePartial({ accountMode: "streamer" })}
              >
                <strong>My own account</strong>
                <span>
                  Replies show as you. Log in with the same Twitch account you
                  stream on — no extra account needed.
                </span>
              </button>
            </div>
            <div className="btn-row">
              <button
                className="btn btn-ghost"
                onClick={() =>
                  setPhase(setupPath === "paste" ? "paste" : "welcome")
                }
              >
                Back
              </button>
              <button
                className="btn btn-primary"
                disabled={busy || !settings.accountMode}
                onClick={async () => {
                  await savePartial({ accountMode: settings.accountMode });
                  afterAccountChoice();
                }}
              >
                {settings.accountMode === "bot"
                  ? "Continue with bot setup"
                  : setupPath === "walkthrough"
                    ? "Continue"
                    : "Connect with Twitch"}
              </button>
            </div>
          </>
        )}

        {phase === "guide" && (
          <GuideSteps
            step={guideStep}
            clientId={settings.clientId}
            onClientId={(v) => setSettings({ ...settings, clientId: v.trim() })}
            accountMode={settings.accountMode === "bot" ? "bot" : "streamer"}
            botLoginHint={botLoginHint}
            onBack={() => {
              if (guideStep === 0) {
                setPhase(
                  settings.accountMode === "bot" ? "bot-create" : "account",
                );
              } else {
                setGuideStep((s) => s - 1);
              }
            }}
            onNext={async () => {
              if (guideStep < GUIDE_LAST_STEP) {
                setError("");
                setGuideStep((s) => s + 1);
                return;
              }
              if (!settings.clientId) {
                setError("Paste your Client ID to continue.");
                return;
              }
              setError("");
              await savePartial({ clientId: settings.clientId });
              afterClientId();
            }}
          />
        )}

        {phase === "paste" && (
          <>
            <p className="lead">Paste your Twitch Client ID below.</p>
            <div className="field">
              <label>Client ID</label>
              <input
                value={settings.clientId}
                onChange={(e) =>
                  setSettings({ ...settings, clientId: e.target.value.trim() })
                }
                placeholder="Looks like: abcdefghijklmnopqrstuvwxyz1234"
                autoFocus
              />
            </div>
            <p className="hint">
              Find it at{" "}
              <button
                type="button"
                className="linkish"
                onClick={() => openUrl(TWITCH_CONSOLE_HOME)}
              >
                Twitch Developer Console → Applications
              </button>
              . Open your app and copy <strong>Client ID</strong> (not the
              secret).
            </p>
            <div className="btn-row" style={{ marginTop: 16 }}>
              <button
                className="btn btn-ghost"
                onClick={() => setPhase("welcome")}
              >
                Back
              </button>
              <button
                className="btn btn-ghost"
                onClick={() => {
                  setSetupPath("walkthrough");
                  setPhase("account");
                }}
              >
                Need help instead?
              </button>
              <button
                className="btn btn-primary"
                disabled={!settings.clientId}
                onClick={async () => {
                  await savePartial({ clientId: settings.clientId });
                  setPhase("account");
                }}
              >
                Continue
              </button>
            </div>
          </>
        )}

        {phase === "bot-create" && (
          <>
            <BotPrepProgress
              step={0}
              includeClientId={setupPath === "walkthrough"}
            />
            <h2 className="guide-title">Create the bot Twitch account</h2>
            <p className="lead">
              The bot needs its own Twitch login — separate from your streamer
              account. Skip ahead if you already have one.
            </p>
            <ol className="guide-list">
              <li>Open signup and register a new account (e.g. YourNameBot).</li>
              <li>Verify email if Twitch asks — you’ll need it to log in.</li>
              <li>Remember that username; you’ll use it next.</li>
            </ol>
            <div className="field">
              <label>Bot username (optional reminder)</label>
              <input
                value={botLoginHint}
                onChange={(e) => setBotLoginHint(e.target.value.trim())}
                placeholder="e.g. techjeeperbot"
              />
            </div>
            <div className="btn-row">
              <button
                className="btn btn-accent"
                onClick={() => openUrl("https://www.twitch.tv/signup")}
              >
                Open Twitch signup
              </button>
            </div>
            <div className="btn-row" style={{ marginTop: 16 }}>
              <button
                className="btn btn-ghost"
                onClick={() => setPhase("account")}
              >
                Back
              </button>
              <button className="btn btn-primary" onClick={afterBotCreate}>
                I have a bot account
              </button>
            </div>
          </>
        )}

        {phase === "bot-mod" && (
          <>
            <BotPrepProgress
              step={setupPath === "walkthrough" ? 2 : 1}
              includeClientId={setupPath === "walkthrough"}
            />
            <h2 className="guide-title">Make the bot a moderator</h2>
            <p className="lead">
              Log into Twitch as your <strong>streamer</strong> account, open
              your chat, and mod the bot so it can send messages reliably.
            </p>
            <div className="guide-card">
              <div className="guide-row">
                <span>In your chat, type</span>
                <strong>/mod {botLoginHint || "BotName"}</strong>
              </div>
            </div>
            <p className="hint">
              You can also add the bot as a mod from Creator Dashboard →
              Community → Roles → Moderators.
            </p>
            <div className="btn-row" style={{ marginTop: 16 }}>
              <button
                className="btn btn-ghost"
                onClick={() =>
                  setPhase(setupPath === "walkthrough" ? "guide" : "bot-create")
                }
              >
                Back
              </button>
              <button
                className="btn btn-primary"
                onClick={() => setPhase("bot-channel")}
              >
                Next
              </button>
            </div>
          </>
        )}

        {phase === "bot-channel" && (
          <>
            <BotPrepProgress
              step={setupPath === "walkthrough" ? 3 : 2}
              includeClientId={setupPath === "walkthrough"}
            />
            <h2 className="guide-title">Your stream channel</h2>
            <p className="lead">
              Enter the channel Streamry should join — your streamer username,
              not the bot’s.
            </p>
            <div className="field">
              <label>Channel (streamer username)</label>
              <input
                value={settings.channel}
                onChange={(e) =>
                  setSettings({ ...settings, channel: e.target.value })
                }
                placeholder="yourchannel"
                autoFocus
              />
            </div>
            <p className="hint">
              Next you’ll authorize while logged into Twitch as{" "}
              <strong>{botLoginHint || "the bot"}</strong>
              {settings.channel
                ? ` — not as ${settings.channel}`
                : " — not as your streamer"}
              .
            </p>
            <div className="btn-row" style={{ marginTop: 16 }}>
              <button
                className="btn btn-ghost"
                onClick={() => setPhase("bot-mod")}
              >
                Back
              </button>
              <button
                className="btn btn-primary"
                disabled={busy || !settings.channel.trim()}
                onClick={async () => {
                  await savePartial({
                    accountMode: "bot",
                    channel: settings.channel.trim(),
                  });
                  await startLogin();
                }}
              >
                Connect as bot
              </button>
            </div>
          </>
        )}

        {phase === "authorize" && device && (
          <>
            <p className="lead">
              Enter this code on Twitch, then click <strong>Authorize</strong>.
            </p>
            {settings.accountMode === "bot" ? (
              <p className="hint" style={{ marginBottom: 12 }}>
                Twitch must be signed in as{" "}
                <strong>{botLoginHint || "your bot"}</strong>
                {settings.channel ? (
                  <>
                    {" "}
                    — not <strong>{settings.channel}</strong>
                  </>
                ) : null}
                . Switch accounts in the browser before authorizing, or replies
                will post as you.
              </p>
            ) : (
              <p className="hint" style={{ marginBottom: 12 }}>
                Authorize while logged in as your streamer account. Replies will
                show under that name.
              </p>
            )}
            <div className="code-box">{device.userCode}</div>
            <div className="btn-row">
              <button
                className="btn btn-accent"
                onClick={() => openUrl(device.verificationUri)}
              >
                Open Twitch again
              </button>
              <button
                className="btn btn-ghost"
                onClick={() =>
                  setPhase(
                    settings.accountMode === "bot"
                      ? "bot-channel"
                      : setupPath === "walkthrough"
                        ? "guide"
                        : "account",
                  )
                }
              >
                Back
              </button>
            </div>
            <p className="hint" style={{ marginTop: 14 }}>
              Waiting for you to authorize…
            </p>
          </>
        )}

        {phase === "done" && (
          <>
            <p className="lead">
              Connected as <strong>{settings.botLogin || "Twitch user"}</strong>
              {settings.channel ? (
                <>
                  {" "}
                  · channel <strong>#{settings.channel}</strong>
                </>
              ) : null}
              . You can import StreamElements later from Settings.
            </p>
            {settings.accountMode === "bot" &&
              settings.botLogin &&
              settings.channel &&
              settings.botLogin.toLowerCase() ===
                settings.channel.toLowerCase() && (
                <p
                  className="hint"
                  style={{ color: "var(--warn)", marginBottom: 12 }}
                >
                  Bot login matches your channel — you authorized as the
                  streamer. Connect again while logged into Twitch as the bot.
                </p>
              )}
            <div className="btn-row">
              {settings.accountMode === "bot" &&
                settings.botLogin &&
                settings.channel &&
                settings.botLogin.toLowerCase() ===
                  settings.channel.toLowerCase() && (
                  <button
                    className="btn btn-ghost"
                    onClick={() => setPhase("bot-create")}
                  >
                    Connect as bot instead
                  </button>
                )}
              <button className="btn btn-primary" onClick={finish}>
                Open Streamry
              </button>
            </div>
          </>
        )}

        {error && (
          <p style={{ color: "var(--danger)", marginTop: 14 }}>{error}</p>
        )}
      </div>
    </div>
  );
}

function BotPrepProgress({
  step,
  includeClientId,
}: {
  step: number;
  includeClientId: boolean;
}) {
  const labels = includeClientId
    ? ["Create bot", "Client ID", "Mod bot", "Channel"]
    : ["Create bot", "Mod bot", "Channel"];
  return (
    <>
      <div className="guide-progress">
        {labels.map((_, i) => (
          <span key={i} className={i <= step ? "on" : ""} />
        ))}
      </div>
      <p className="guide-step-label">
        Bot setup · Step {step + 1} of {labels.length}
        {labels[step] ? ` · ${labels[step]}` : ""}
      </p>
    </>
  );
}

function GuideSteps({
  step,
  clientId,
  onClientId,
  accountMode,
  botLoginHint,
  onBack,
  onNext,
}: {
  step: number;
  clientId: string;
  onClientId: (v: string) => void;
  accountMode: "bot" | "streamer";
  botLoginHint: string;
  onBack: () => void;
  onNext: () => void;
}) {
  const loginAs =
    accountMode === "bot"
      ? botLoginHint || "your bot account"
      : "your streamer account";

  const steps = [
    {
      title: "Open the Twitch Developer Console",
      body: (
        <>
          <p className="lead">
            You’ll land on the Console dashboard (Extensions, Organizations,
            Applications). That’s expected.
          </p>
          <ol className="guide-list">
            <li>Click the button below to open Twitch Developers.</li>
            <li>
              Log in as <strong>{loginAs}</strong>
              {accountMode === "bot"
                ? " — not your streamer account."
                : " if asked."}
            </li>
            <li>
              You should see an <strong>Applications</strong> section on the
              dashboard.
            </li>
          </ol>
          <button
            className="btn btn-accent"
            onClick={() => openUrl(TWITCH_CONSOLE)}
          >
            Open Twitch Developer Console
          </button>
        </>
      ),
    },
    {
      title: "Register your application",
      body: (
        <>
          <p className="lead">
            On the dashboard, under <strong>Applications</strong>, click{" "}
            <strong>Register Your Application</strong>. Then fill the form with
            these values:
          </p>
          <div className="guide-card">
            <div className="guide-row">
              <span>Name</span>
              <strong>Pick any name you like</strong>
            </div>
            <div className="guide-row">
              <span>OAuth Redirect URL</span>
              <strong>https://localhost</strong>
            </div>
            <div className="guide-row">
              <span>Category</span>
              <strong>Chat Bot</strong>
            </div>
            <div className="guide-row">
              <span>Client Type</span>
              <strong>Public</strong>
            </div>
          </div>
          <ol className="guide-list">
            <li>
              Choose <strong>Public</strong>, not Confidential.
            </li>
            <li>
              The name is only a Console label — it does not affect chat.
            </li>
            <li>
              When the form is complete, create the application at the bottom of
              the page.
            </li>
            <li>
              Already have an app? Skip ahead — find it in your Applications
              list.
            </li>
          </ol>
          <div className="btn-row">
            <button
              className="btn btn-accent"
              onClick={() => openUrl(TWITCH_REGISTER)}
            >
              Open register form
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => openUrl(TWITCH_CONSOLE)}
            >
              Open dashboard
            </button>
          </div>
        </>
      ),
    },
    {
      title: "Copy the Client ID",
      body: (
        <>
          <p className="lead">
            Back on the Console, find your app under Applications and click{" "}
            <strong>Manage</strong>.
          </p>
          <ol className="guide-list">
            <li>
              Copy <strong>Client ID</strong> only — do not copy Client Secret.
            </li>
            <li>Client ID is a long string of letters and numbers.</li>
          </ol>
          <button
            className="btn btn-ghost"
            onClick={() => openUrl(TWITCH_CONSOLE_HOME)}
          >
            Open my applications list
          </button>
        </>
      ),
    },
    {
      title: "Paste it here",
      body: (
        <>
          <p className="lead">Paste the Client ID you copied from Twitch.</p>
          <div className="field">
            <label>Client ID</label>
            <input
              value={clientId}
              onChange={(e) => onClientId(e.target.value)}
              placeholder="Paste here"
              autoFocus
            />
          </div>
        </>
      ),
    },
  ];

  const current = steps[step];

  return (
    <>
      {accountMode === "bot" && (
        <BotPrepProgress step={1} includeClientId />
      )}
      <div className="guide-progress">
        {steps.map((_, i) => (
          <span key={i} className={i <= step ? "on" : ""} />
        ))}
      </div>
      <p className="guide-step-label">
        {accountMode === "bot" ? "Client ID" : "Getting started"} · Step{" "}
        {step + 1} of {steps.length}
      </p>
      <h2 className="guide-title">{current.title}</h2>
      {current.body}
      <div className="btn-row" style={{ marginTop: 20 }}>
        <button className="btn btn-ghost" onClick={onBack}>
          Back
        </button>
        <button className="btn btn-primary" onClick={onNext}>
          {step === steps.length - 1 ? "Save & continue" : "Next"}
        </button>
      </div>
    </>
  );
}
