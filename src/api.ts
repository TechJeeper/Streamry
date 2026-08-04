import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveGiveaway,
  AppSettings,
  Automation,
  ChatCommand,
  ChatTimer,
  CustomVariable,
  DeviceCode,
  Giveaway,
  GiveawayRunHistory,
  ImportResult,
  MediaClip,
  OverlayInfo,
  RuntimeStatus,
  SePreview,
  StreamDeckStatus,
  UpdateCheck,
} from "./types";

export const api = {
  getStatus: () => invoke<RuntimeStatus>("get_status"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke("save_settings", { settings }),
  startDeviceLogin: (scopes: string[]) =>
    invoke<DeviceCode>("start_device_login", { scopes }),
  logout: () => invoke("logout"),
  connectBot: () => invoke("connect_bot"),
  disconnectBot: () => invoke("disconnect_bot"),
  sendChat: (message: string) => invoke("send_chat", { message }),
  listCommands: () => invoke<ChatCommand[]>("list_commands"),
  upsertCommand: (cmd: ChatCommand) => invoke<ChatCommand>("upsert_command", { cmd }),
  deleteCommand: (id: string) => invoke("delete_command", { id }),
  listTimers: () => invoke<ChatTimer[]>("list_timers"),
  upsertTimer: (timer: ChatTimer) => invoke<ChatTimer>("upsert_timer", { timer }),
  deleteTimer: (id: string) => invoke("delete_timer", { id }),
  listGiveaways: () => invoke<Giveaway[]>("list_giveaways"),
  upsertGiveaway: (gw: Giveaway) => invoke<Giveaway>("upsert_giveaway", { gw }),
  deleteGiveaway: (id: string) => invoke("delete_giveaway", { id }),
  getActiveGiveaway: () => invoke<ActiveGiveaway | null>("get_active_giveaway"),
  listGiveawayHistory: (limit?: number) =>
    invoke<GiveawayRunHistory[]>("list_giveaway_history", { limit }),
  startGiveaway: (id: string) => invoke("start_giveaway", { id }),
  stopGiveaway: () => invoke("stop_giveaway"),
  drawGiveaway: () =>
    invoke<{ userId: string; login: string }[]>("draw_giveaway"),
  listAutomations: () => invoke<Automation[]>("list_automations"),
  upsertAutomation: (auto: Automation) =>
    invoke<Automation>("upsert_automation", { auto }),
  deleteAutomation: (id: string) => invoke("delete_automation", { id }),
  getOverlayInfo: () => invoke<OverlayInfo>("get_overlay_info"),
  listMedia: () => invoke<MediaClip[]>("list_media"),
  importMedia: (path: string, name?: string) =>
    invoke<MediaClip>("import_media", { path, name: name ?? null }),
  upsertMedia: (clip: MediaClip) => invoke<MediaClip>("upsert_media", { clip }),
  deleteMedia: (id: string) => invoke("delete_media", { id }),
  testMedia: (id: string) => invoke("test_media", { id }),
  listVariables: () => invoke<CustomVariable[]>("list_variables"),
  upsertVariable: (var_: CustomVariable) =>
    invoke<CustomVariable>("upsert_variable", { var: var_ }),
  deleteVariable: (id: string) => invoke("delete_variable", { id }),
  parseSeZip: (path: string) => invoke<SePreview>("parse_streamelements_zip", { path }),
  importSe: (
    path: string,
    commandIds: string[],
    timerIds: string[],
    variableIds: string[],
    automationIds: string[],
    onCollision: string,
  ) =>
    invoke<ImportResult>("import_streamelements", {
      path,
      commandIds,
      timerIds,
      variableIds,
      automationIds,
      onCollision,
    }),
  exportBackup: (path: string) => invoke("export_backup", { path }),
  previewBackup: (path: string) =>
    invoke<{
      commands: number;
      timers: number;
      giveaways: number;
      automations: number;
      variables: number;
      exportedAt: string;
    }>("preview_backup", { path }),
  restoreBackup: (args: {
    path: string;
    includeCommands: boolean;
    includeTimers: boolean;
    includeGiveaways: boolean;
    includeAutomations: boolean;
    includeVariables: boolean;
    replace: boolean;
  }) => invoke("restore_backup", args),
  completeSetup: () => invoke("complete_setup"),
  getAppVersion: () => invoke<string>("get_app_version"),
  checkForUpdate: () => invoke<UpdateCheck>("check_for_update"),
  dismissUpdate: (version: string) => invoke("dismiss_update", { version }),
  resetApp: () => invoke("reset_app"),
  checkAppName: (name: string) =>
    invoke<{
      ok: boolean;
      status: string;
      message: string;
      suggested?: string | null;
    }>("check_app_name", { name }),
  getStreamDeckStatus: () => invoke<StreamDeckStatus>("get_streamdeck_status"),
  installStreamDeckPlugin: () =>
    invoke<StreamDeckStatus>("install_streamdeck_plugin"),
  setStreamDeckControl: (enabled: boolean) =>
    invoke<StreamDeckStatus>("set_streamdeck_control", { enabled }),
};
