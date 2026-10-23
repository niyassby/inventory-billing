/** The file's bytes as base64 (no `data:` prefix), for upload commands. */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      resolve((typeof reader.result === "string" ? reader.result : "").replace(/^data:[^,]*,/, ""));
    reader.onerror = () => reject(reader.error ?? new Error("The file couldn't be read."));
    reader.readAsDataURL(file);
  });
}
