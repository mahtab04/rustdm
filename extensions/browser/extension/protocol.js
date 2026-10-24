export const HOST = "com.rustdm.bridge";

export function fileUrl(value) {
  const url = new URL(value);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new Error("Use a direct HTTP/HTTPS URL without embedded credentials.");
  }
  url.hash = "";
  if (new TextEncoder().encode(url.href).length > 16384) throw new Error("URL is too long.");
  return url.href;
}

export function validateFilename(value) {
  const bytes = new TextEncoder().encode(value).length;
  if (!bytes || bytes > 180 || [".", ".."].includes(value) || /[. ]$/.test(value) ||
      /[<>:"/\\|?*\u0000-\u001f\u007f-\u009f]/u.test(value)) {
    throw new Error("Choose a filename of up to 180 UTF-8 bytes, without paths or reserved characters.");
  }
  const stem = value.split(".")[0].replace(/ +$/, "");
  if (/^(CON|PRN|AUX|NUL|COM[1-9¹²³]|LPT[1-9¹²³])$/i.test(stem)) {
    throw new Error("That filename is reserved by Windows.");
  }
  return value;
}

export function suggestFilename(value) {
  try {
    const path = new URL(fileUrl(value)).pathname.split("/").pop();
    const name = decodeURIComponent(path || "download.bin").replace(/[<>:"/\\|?*\u0000-\u001f\u007f-\u009f]/gu, "_").replace(/[. ]+$/, "");
    validateFilename(name);
    return name;
  } catch { return "download.bin"; }
}

export function request(url, filename, id = crypto.randomUUID()) {
  return { version: 1, id, action: "enqueue", url: fileUrl(url), filename: validateFilename(filename) };
}

export function response(value, id) {
  if (value?.version !== 1 || value.id !== id || typeof value.ok !== "boolean") {
    throw new Error("Unexpected response from RustDM.");
  }
  if (!value.ok) throw new Error(value.error || "RustDM could not queue the file.");
  if (!Number.isSafeInteger(value.job_id) || value.job_id <= 0 || typeof value.output !== "string") {
    throw new Error("Invalid queue response from RustDM.");
  }
  return value;
}
