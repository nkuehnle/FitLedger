/** Android WebView (file saving goes through the share sheet there; alarms are native). */
export const IS_ANDROID = /Android/i.test(navigator.userAgent);

/** Base64 of a picked file (for sending bytes to Rust). */
export async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}
