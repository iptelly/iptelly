export const isInputFocused = () => {
  var activeElement = document.activeElement;
  var inputs = ["input", "select", "button", "textarea"];
  return activeElement && inputs.indexOf(activeElement.tagName?.toLowerCase()) !== -1;
};

export const sanitizeFileName = (fileName: string) => {
  return (
    fileName
      .replace(/[/\\:*?"<>|]/g, "_")
      .replace(/[\x00-\x1F\x7F]/g, "")
      .replace(/^\.+/, "")
      .replace(/\.+$/, "")
      .replace(/^\s+|\s+$/g, "") || "untitled"
  );
};

export const getDateFormatted = (): string => {
  return new Date().toISOString().replace(/T|:/g, "-").split(".")[0];
};

// Only the last path segment can hold the file's extension - dots in the
// host, query string or fragment don't count. PHP endpoints (e.g. Xtream's
// get.php) serve the stream rather than a file of that type, so they fall
// back to mp4 like URLs with no extension at all.
export const getExtension = (url: string): string => {
  const path = url.replace(/^[a-z][a-z\d+.-]*:\/\/[^/?#]*/i, "").split(/[?#]/)[0];
  const fileName = path.substring(path.lastIndexOf("/") + 1);
  const dot = fileName.lastIndexOf(".");
  const extension = dot === -1 ? "" : fileName.substring(dot + 1);
  if (!extension || extension.toLowerCase() === "php") return "mp4";
  return extension;
};

export const formatBytes = (bytes?: number): string => {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${exponent === 0 ? value : value.toFixed(1)} ${units[exponent]}`;
};
