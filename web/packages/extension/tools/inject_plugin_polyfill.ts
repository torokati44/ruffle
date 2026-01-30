import { replaceInFileSync } from "replace-in-file";

// Search-and-replace the manual polyfill injection with the actual code it
// needs to insert.
const pluginPolyfillSource = Deno
    .readTextFileSync("assets/dist/pluginPolyfill.js")
    .replaceAll("\r", "\\r")
    .replaceAll("\n", "\\n")
    .replaceAll('"', '\\"');

replaceInFileSync({
    files: "./assets/dist/content.js",
    from: [/%PLUGIN_POLYFILL_SOURCE%/g],
    to: pluginPolyfillSource,
});
