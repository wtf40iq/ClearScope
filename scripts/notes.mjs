// Prints the CHANGELOG.md section for the version in package.json.
// Used as the GitHub release text, which the app shows in the update dialog.
import { readFileSync } from "node:fs";

const { version } = JSON.parse(readFileSync("package.json", "utf8"));
const log = readFileSync("CHANGELOG.md", "utf8").split(/\r?\n/);
const start = log.findIndex((l) => l.startsWith("## ") && l.includes(version));
if (start < 0) {
  console.log(`ClearScope ${version}`);
} else {
  const rest = log.slice(start + 1);
  const end = rest.findIndex((l) => l.startsWith("## "));
  console.log(rest.slice(0, end < 0 ? undefined : end).join("\n").trim());
}
