import { replaceInFileSync } from "replace-in-file";

const bundledTexts: { [name: string]: { [key: string]: string } } = {};
const locales: string[] = [];

for (const entry of Deno.readDirSync("texts")) {
    if (entry.isDirectory) {
        locales.push(entry.name);
    }
}

// For build reproducibility, sort the locales to make sure we don't accidentally rearrange them on different machines.
// The actual order isn't important, just that it's the same.
locales.sort();

locales.forEach((locale) => {
    const files: string[] = [];
    for (const entry of Deno.readDirSync("texts/" + locale)) {
        if (entry.isFile && entry.name.endsWith(".ftl")) {
            files.push(entry.name);
        }
    }
    files.sort();
    if (files.length > 0) {
        bundledTexts[locale] = {};
        files.forEach((filename) => {
            bundledTexts[locale]![filename] = Deno
                .readTextFileSync("texts/" + locale + "/" + filename)
                .replaceAll("\r\n", "\n");
        });
    }
});

const options = {
    files: "dist/**",
    from: [/\{\s*\/\*\s*%BUNDLED_TEXTS%\s*\*\/\s*}/g],
    to: [JSON.stringify(bundledTexts, null, 2)],
};

replaceInFileSync(options);
