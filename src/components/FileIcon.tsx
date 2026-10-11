import { File, FileArchive, FileAudio, FileCode, FileImage, FileText, FileVideo, Cog, Gamepad2, Database, FileCog } from "lucide-react";
import { categoryOf } from "../categories";

/** Two-tone folder in the spirit of Windows 11 Explorer. */
export function FolderIcon({ size = 16, open = false }: { size?: number; open?: boolean }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className="ic ic-folder" aria-hidden>
      <path d="M1.5 3.6c0-.6.5-1.1 1.1-1.1h3.3c.3 0 .6.1.8.3l1.2 1.2h5.5c.6 0 1.1.5 1.1 1.1v7.3c0 .6-.5 1.1-1.1 1.1H2.6c-.6 0-1.1-.5-1.1-1.1z" fill="var(--folder-back)" />
      {open
        ? <path d="M3.1 6.6c.1-.5.6-.9 1.1-.9h10c.7 0 1.2.6 1.1 1.3l-.9 5.4c-.1.5-.6.9-1.1.9H2.4c-.6 0-1-.5-.9-1.1z" fill="var(--folder-front)" />
        : <path d="M1.5 6.1c0-.6.5-1.1 1.1-1.1h10.8c.6 0 1.1.5 1.1 1.1v6.3c0 .6-.5 1.1-1.1 1.1H2.6c-.6 0-1.1-.5-1.1-1.1z" fill="var(--folder-front)" />}
      <path d="M2.6 5.5h10.8" stroke="#fff" strokeOpacity=".35" strokeWidth=".8" transform={open ? "translate(1.4 .7)" : undefined} />
    </svg>
  );
}

export default function FileIcon({ isDir, ext, size = 15, open = false }: { isDir: boolean; ext: string; size?: number; open?: boolean }) {
  if (isDir) return <FolderIcon size={size + 1} open={open} />;
  const c = categoryOf(ext);
  const props = { size, className: "ic", style: { color: c.color }, strokeWidth: 1.8 };
  switch (c.key) {
    case "video": return <FileVideo {...props} />;
    case "image": return <FileImage {...props} />;
    case "audio": return <FileAudio {...props} />;
    case "archive": return <FileArchive {...props} />;
    case "doc": return <FileText {...props} />;
    case "code": return <FileCode {...props} />;
    case "exe": return <Cog {...props} />;
    case "system": return <FileCog {...props} />;
    case "game": return <Gamepad2 {...props} />;
    case "db": return <Database {...props} />;
    default: return <File {...props} />;
  }
}
