import streamDeck from "@elgato/streamdeck";
import {
  ConnectBotAction,
  DisconnectBotAction,
  DrawGiveawayAction,
  PlayMediaAction,
  RunAutomationAction,
  RunCommandAction,
  SendChatAction,
  StartGiveawayAction,
  StopGiveawayAction,
} from "./actions/actions";
import { apiJson } from "./api";

streamDeck.actions.registerAction(new RunCommandAction());
streamDeck.actions.registerAction(new RunAutomationAction());
streamDeck.actions.registerAction(new StartGiveawayAction());
streamDeck.actions.registerAction(new StopGiveawayAction());
streamDeck.actions.registerAction(new DrawGiveawayAction());
streamDeck.actions.registerAction(new PlayMediaAction());
streamDeck.actions.registerAction(new SendChatAction());
streamDeck.actions.registerAction(new ConnectBotAction());
streamDeck.actions.registerAction(new DisconnectBotAction());

type ListKind = "commands" | "automations" | "giveaways" | "media";

streamDeck.ui.onSendToPlugin(async (ev) => {
  const payload = ev.payload as { event?: string; kind?: ListKind };
  if (payload?.event !== "list" || !payload.kind) return;
  try {
    const path =
      payload.kind === "commands"
        ? "/api/v1/commands"
        : payload.kind === "automations"
          ? "/api/v1/automations"
          : payload.kind === "giveaways"
            ? "/api/v1/giveaways"
            : "/api/v1/media";
    const items = await apiJson<
      { id: string; name?: string; title?: string; enabled?: boolean }[]
    >("GET", path);
    await streamDeck.ui.sendToPropertyInspector({
      event: "listResult",
      kind: payload.kind,
      items: items.map((i) => ({
        id: i.id,
        label: i.name || i.title || i.id,
      })),
    });
  } catch (e) {
    await streamDeck.ui.sendToPropertyInspector({
      event: "listError",
      error: e instanceof Error ? e.message : String(e),
    });
  }
});

void streamDeck.connect();
