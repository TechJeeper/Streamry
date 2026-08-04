import {
  action,
  KeyDownEvent,
  SingletonAction,
  WillAppearEvent,
} from "@elgato/streamdeck";
import { apiFetch } from "../api";

type EntitySettings = {
  entityId?: string;
  entityLabel?: string;
};

async function runOrAlert(
  ev: KeyDownEvent<EntitySettings>,
  fn: () => Promise<void>,
): Promise<void> {
  try {
    await fn();
  } catch {
    await ev.action.showAlert();
  }
}

@action({ UUID: "com.streamry.streamdeck.command.run" })
export class RunCommandAction extends SingletonAction<EntitySettings> {
  override async onWillAppear(ev: WillAppearEvent<EntitySettings>): Promise<void> {
    const label = ev.payload.settings.entityLabel;
    if (label) await ev.action.setTitle(label);
  }

  override async onKeyDown(ev: KeyDownEvent<EntitySettings>): Promise<void> {
    const id = ev.payload.settings.entityId;
    if (!id) {
      await ev.action.showAlert();
      return;
    }
    await runOrAlert(ev, async () => {
      await apiFetch("POST", `/api/v1/commands/${encodeURIComponent(id)}/run`);
    });
  }
}

@action({ UUID: "com.streamry.streamdeck.automation.run" })
export class RunAutomationAction extends SingletonAction<EntitySettings> {
  override async onWillAppear(ev: WillAppearEvent<EntitySettings>): Promise<void> {
    const label = ev.payload.settings.entityLabel;
    if (label) await ev.action.setTitle(label);
  }

  override async onKeyDown(ev: KeyDownEvent<EntitySettings>): Promise<void> {
    const id = ev.payload.settings.entityId;
    if (!id) {
      await ev.action.showAlert();
      return;
    }
    await runOrAlert(ev, async () => {
      await apiFetch("POST", `/api/v1/automations/${encodeURIComponent(id)}/run`);
    });
  }
}

@action({ UUID: "com.streamry.streamdeck.giveaway.start" })
export class StartGiveawayAction extends SingletonAction<EntitySettings> {
  override async onWillAppear(ev: WillAppearEvent<EntitySettings>): Promise<void> {
    const label = ev.payload.settings.entityLabel;
    if (label) await ev.action.setTitle(label);
  }

  override async onKeyDown(ev: KeyDownEvent<EntitySettings>): Promise<void> {
    const id = ev.payload.settings.entityId;
    if (!id) {
      await ev.action.showAlert();
      return;
    }
    await runOrAlert(ev, async () => {
      await apiFetch("POST", `/api/v1/giveaways/${encodeURIComponent(id)}/start`);
    });
  }
}

@action({ UUID: "com.streamry.streamdeck.giveaway.stop" })
export class StopGiveawayAction extends SingletonAction {
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    try {
      await apiFetch("POST", "/api/v1/giveaways/stop");
    } catch {
      await ev.action.showAlert();
    }
  }
}

@action({ UUID: "com.streamry.streamdeck.giveaway.draw" })
export class DrawGiveawayAction extends SingletonAction {
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    try {
      await apiFetch("POST", "/api/v1/giveaways/draw");
    } catch {
      await ev.action.showAlert();
    }
  }
}

@action({ UUID: "com.streamry.streamdeck.media.play" })
export class PlayMediaAction extends SingletonAction<EntitySettings> {
  override async onWillAppear(ev: WillAppearEvent<EntitySettings>): Promise<void> {
    const label = ev.payload.settings.entityLabel;
    if (label) await ev.action.setTitle(label);
  }

  override async onKeyDown(ev: KeyDownEvent<EntitySettings>): Promise<void> {
    const id = ev.payload.settings.entityId;
    if (!id) {
      await ev.action.showAlert();
      return;
    }
    await runOrAlert(ev, async () => {
      await apiFetch("POST", `/api/v1/media/${encodeURIComponent(id)}/play`);
    });
  }
}

type ChatSettings = { message?: string };

@action({ UUID: "com.streamry.streamdeck.chat.send" })
export class SendChatAction extends SingletonAction<ChatSettings> {
  override async onWillAppear(ev: WillAppearEvent<ChatSettings>): Promise<void> {
    const msg = ev.payload.settings.message?.trim();
    if (msg) await ev.action.setTitle(msg.length > 12 ? `${msg.slice(0, 12)}…` : msg);
  }

  override async onKeyDown(ev: KeyDownEvent<ChatSettings>): Promise<void> {
    const message = ev.payload.settings.message?.trim();
    if (!message) {
      await ev.action.showAlert();
      return;
    }
    try {
      await apiFetch("POST", "/api/v1/chat", { message });
    } catch {
      await ev.action.showAlert();
    }
  }
}

@action({ UUID: "com.streamry.streamdeck.bot.connect" })
export class ConnectBotAction extends SingletonAction {
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    try {
      await apiFetch("POST", "/api/v1/bot/connect");
    } catch {
      await ev.action.showAlert();
    }
  }
}

@action({ UUID: "com.streamry.streamdeck.bot.disconnect" })
export class DisconnectBotAction extends SingletonAction {
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    try {
      await apiFetch("POST", "/api/v1/bot/disconnect");
    } catch {
      await ev.action.showAlert();
    }
  }
}
