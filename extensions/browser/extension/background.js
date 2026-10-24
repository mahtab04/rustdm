import { openConfirmation } from "./bridge.js";
const api = globalThis.browser ?? chrome;
const MENU = "rustdm-queue-link";
api.runtime.onInstalled.addListener(() => {
  api.contextMenus.removeAll().then(() => api.contextMenus.create({
    id: MENU, title: "Queue link in RustDM...", contexts: ["link"],
    targetUrlPatterns: ["http://*/*", "https://*/*"],
  }));
});
api.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId === MENU && info.linkUrl) {
    openConfirmation(api, info.linkUrl).catch(() => {
      api.action.setBadgeText({ text: "!" });
      api.action.setTitle({ title: "Could not open RustDM review. Click to paste the URL manually." });
    });
  }
});
