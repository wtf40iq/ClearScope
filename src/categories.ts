export interface Category {
  key: string;
  label: string;
  color: string;
}

// Calm, print-like palette that reads well on light and dark backgrounds.
export const CATEGORIES: Category[] = [
  { key: "video", label: "Видео", color: "#5b7fd6" },
  { key: "image", label: "Изображения", color: "#cf6f94" },
  { key: "audio", label: "Аудио", color: "#d99a52" },
  { key: "archive", label: "Архивы и образы", color: "#c4ae4f" },
  { key: "doc", label: "Документы", color: "#4a9fc4" },
  { key: "code", label: "Код", color: "#4aa889" },
  { key: "exe", label: "Программы", color: "#d27258" },
  { key: "system", label: "Системные", color: "#8990a0" },
  { key: "game", label: "Игровые данные", color: "#7ea85f" },
  { key: "db", label: "Базы и кэш", color: "#9580c9" },
  { key: "other", label: "Прочее", color: "#737b8c" },
];

const MAP: Record<string, string> = {};
const add = (cat: string, exts: string) => exts.split(" ").forEach((e) => (MAP[e] = cat));
add("video", "mp4 mkv avi mov wmv webm flv m4v mpg mpeg ts m2ts 3gp vob");
add("image", "jpg jpeg png gif bmp webp heic heif tif tiff raw cr2 nef arw dng psd svg ico avif");
add("audio", "mp3 flac wav aac ogg m4a wma opus aiff alac mid");
add("archive", "zip rar 7z tar gz bz2 xz zst iso img vhd vhdx vmdk cab msu wim esd dmg tgz");
add("doc", "pdf doc docx xls xlsx ppt pptx txt rtf odt ods odp md csv epub djvu fb2 one");
add("code", "js ts tsx jsx rs py c cpp h hpp cs java go rb php html css scss json xml yaml yml toml lock map kt swift lua sh ps1 bat rlib rmeta d");
add("exe", "exe dll msi msp sys drv ocx com scr appx msix so dylib node");
add("system", "dat log etl evtx tmp bak old cat mui manifest inf pnf regtrans-ms blf");
add("game", "archive pak upk uasset umap bundle assets ress vpk bsa ba2 wad pck forge bin arc rpf big");
add("db", "db sqlite sqlite3 ldb mdb accdb edb idx pf cache sst");

const BY_KEY: Record<string, Category> = Object.fromEntries(CATEGORIES.map((c) => [c.key, c]));

export function categoryOf(ext: string): Category {
  return BY_KEY[MAP[ext] ?? "other"];
}

export function colorOf(ext: string): string {
  return categoryOf(ext).color;
}
