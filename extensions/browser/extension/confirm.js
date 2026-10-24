import { enqueue, takeDraft } from "./bridge.js";
import { suggestFilename } from "./protocol.js";
const api = globalThis.browser ?? chrome;
const url = document.querySelector("#url");
const filename = document.querySelector("#filename");
const status = document.querySelector("#status");
const submit = document.querySelector("#submit");
try {
  url.value = await takeDraft(api, new URL(location.href).searchParams.get("draft"));
  if (url.value) filename.value = suggestFilename(url.value);
} catch { status.textContent = "The selected link is unavailable. Paste a direct file URL."; }
url.addEventListener("change", () => { if (!filename.value) filename.value = suggestFilename(url.value); });
document.querySelector("#download").addEventListener("submit", async (event) => {
  event.preventDefault();
  submit.disabled = true;
  status.textContent = "Adding to queue...";
  try {
    const result = await enqueue(api, url.value, filename.value);
    status.textContent = `Queued as job ${result.job_id}: ${result.output}. Open RustDM to start the queue.`;
    // Keep the form disabled after success to prevent accidental duplicate clicks.
    for (const input of [url, filename]) input.disabled = true;
  } catch (error) {
    status.textContent = `${error.message} Check browser host setup. If the connection failed, inspect the RustDM queue before trying again.`;
    submit.disabled = false;
  }
});
