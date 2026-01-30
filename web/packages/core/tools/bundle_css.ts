import { replaceInFileSync } from "replace-in-file";
import postcss from "postcss";
import cssnanoPlugin from "cssnano";
// @ts-expect-error TS7016 We don't care about the types, if it doesn't work that's fine
import postcssNesting from "postcss-nesting";

const originalCss = Deno.readTextFileSync("src/internal/ui/static-styles.css")
    .replaceAll("\r", "");

const processor = postcss([
    postcssNesting,
    cssnanoPlugin({
        preset: ["advanced", { autoprefixer: { add: true } }],
    }),
]);
processor
    .process(originalCss, { from: "src/internal/ui/static-styles.css" })
    .then((result) => {
        replaceInFileSync({
            files: "dist/**",
            from: [/"\s*\/\*\s*%STATIC_STYLES_CSS%\s*\*\/\s*"/g],
            to: [JSON.stringify(result.css)],
        });
    });
