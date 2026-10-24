import { HOST, fileUrl, request, response } from "./protocol.js";

export async function enqueue(api, url, filename) {
  const message = request(url, filename);
  return response(await api.runtime.sendNativeMessage(HOST, message), message.id);
}

export async function openConfirmation(api, url) {
  const key = `draft:${crypto.randomUUID()}`;
  await api.storage.session.set({ [key]: { url: fileUrl(url), created: Date.now() } });
  try {
    await api.tabs.create({ url: `${api.runtime.getURL("confirm.html")}?draft=${encodeURIComponent(key)}` });
  } catch (error) {
    await api.storage.session.remove(key);
    throw error;
  }
}

export async function takeDraft(api, key) {
  if (!key?.startsWith("draft:")) return "";
  const draft = (await api.storage.session.get(key))[key];
  await api.storage.session.remove(key);
  return draft && Date.now() - draft.created < 10 * 60 * 1000 ? fileUrl(draft.url) : "";
}
