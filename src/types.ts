export type RuntimeStatus = {
  connected: boolean;
  connecting: boolean;
  botLogin?: string | null;
  channel?: string | null;
  live: boolean;
  lastError?: string | null;
  chatLines: number;
  setupComplete: boolean;
};

export type AppSettings = {
  clientId: string;
  channel: string;
  botLogin: string;
  accountMode: string;
  setupComplete: boolean;
  confirmGiveawayEntry: boolean;
  timersLiveOnly: boolean;
  theme: string;
  streamDeckControlEnabled?: boolean;
  streamDeckControlPort?: number;
  streamDeckToken?: string;
};

export type StreamDeckStatus = {
  installed: boolean;
  installPath?: string | null;
  controlEnabled: boolean;
  controlPort: number;
  controlRunning: boolean;
  hasToken: boolean;
  supported: boolean;
  message: string;
};

export type ChatCommand = {
  id: string;
  name: string;
  aliases: string;
  response: string;
  enabled: boolean;
  permission: string;
  globalCooldown: number;
  userCooldown: number;
  mediaId?: string | null;
};

export type MediaClip = {
  id: string;
  name: string;
  mediaType: string;
  fileName: string;
  durationMs: number;
  volume: number;
  /** 16×9 overlay grid rect (0-based). */
  overlayX?: number;
  overlayY?: number;
  overlayW?: number;
  overlayH?: number;
  alwaysShow?: boolean;
  /** Hex `#RRGGBB` to key out; empty = off. */
  chromaKey?: string;
  chromaTolerance?: number;
};

export type ActivityEntry = {
  id: string;
  at: string;
  kind: string;
  title: string;
  detail: string;
  path: string;
  entityId?: string | null;
};

export type OverlayInfo = {
  url: string;
  port: number;
  running: boolean;
};

export type ChatTimer = {
  id: string;
  name: string;
  message: string;
  intervalMins: number;
  minChatLines: number;
  enabled: boolean;
  liveOnly: boolean;
};

export type Giveaway = {
  id: string;
  title: string;
  prize: string;
  entryCommand: string;
  drawCommand: string;
  durationMins: number | null;
  winnerCount: number;
  eligibility: string;
  excludeMods: boolean;
  confirmEntry: boolean;
  announceTemplate: string;
  enabled: boolean;
};

export type ActiveGiveaway = {
  giveaway: Giveaway;
  runId: string;
  status: string;
  startedAt: string;
  endsAt?: string | null;
  entryCount: number;
  winners: { userId: string; login: string }[];
};

export type GiveawayRunHistory = {
  runId: string;
  giveawayId: string;
  title: string;
  prize: string;
  startedAt: string;
  endsAt?: string | null;
  entryCount: number;
  winners: { userId: string; login: string }[];
};

export type Automation = {
  id: string;
  name: string;
  triggerType: string;
  actionType: string;
  actionPayload: string;
  enabled: boolean;
  cooldownSecs: number;
};

export type CustomVariable = {
  id: string;
  name: string;
  value: string;
};

export type DeviceCode = {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  interval: number;
  expiresIn: number;
};

export type SePreview = {
  commands: { id: string; name: string; response: string; enabled: boolean }[];
  timers: { id: string; name: string; message: string; interval: number; enabled: boolean }[];
  variables: { id: string; name: string; value: string }[];
  automations: {
    id: string;
    name: string;
    triggerType: string;
    actionPayload: string;
    enabled: boolean;
  }[];
};

export type ImportResult = {
  importedCommands: number;
  importedTimers: number;
  importedGiveaways: number;
  importedAutomations: number;
  importedVariables: number;
  skipped: number;
};

export type UpdateCheck = {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  dismissed: boolean;
  downloadUrl: string;
  notes?: string | null;
};
