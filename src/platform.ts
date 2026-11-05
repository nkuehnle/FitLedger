import { save } from "@tauri-apps/plugin-dialog";
import { api } from "./api";

/** Android WebView (file saving goes through the share sheet there; alarms are native). */
export const IS_ANDROID = /Android/i.test(navigator.userAgent);

/** Base64 of a picked file (for sending bytes to Rust). */
export async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

/** Hands generated text to the user as a file: the share sheet on Android, a save dialog on
 *  desktop. Returns the saved path, or null if nothing was saved there. */
export async function saveOrShareText(name: string, mime: string, text: string): Promise<string | null> {
  if (IS_ANDROID) {
    await api.shareText(name, mime, text);
    return null;
  }
  const ext = name.split(".").pop()!;
  const path = await save({ defaultPath: name, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return null;
  await api.saveText(path, text);
  return path;
}
