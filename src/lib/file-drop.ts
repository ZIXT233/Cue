export const MAX_DROP_FILES = 50;
export const MAX_DROP_FILE_BYTES = 25 * 1024 * 1024;
export const MAX_DROP_TOTAL_BYTES = 100 * 1024 * 1024;

export function isFileDrag(data: Pick<DataTransfer, "types">): boolean {
  return Array.from(data.types).includes("Files");
}

/** Read synchronously during drop, before the browser protects the data store. */
export function droppedFiles(data: DataTransfer): File[] {
  if (Array.from(data.items).some((item) => item.webkitGetAsEntry?.()?.isDirectory)) {
    throw new Error("files.dropDirectories");
  }
  return Array.from(data.files);
}

export function dropFilesError(files: Pick<File, "size">[]): string | null {
  if (files.length > MAX_DROP_FILES || files.some((file) => file.size > MAX_DROP_FILE_BYTES)
    || files.reduce((sum, file) => sum + file.size, 0) > MAX_DROP_TOTAL_BYTES) return "files.dropLimits";
  return null;
}
