import { replaceInFileSync } from "replace-in-file";
import { join } from "jsr:@std/path@1";

let buildDate = new Date().toISOString();
let versionNumber = Deno.env.get("npm_package_version") ?? "";
let versionChannel = Deno.env.get("CFG_RELEASE_CHANNEL") || "local";
const firefoxExtensionId =
    Deno.env.get("FIREFOX_EXTENSION_ID") || "ruffle@ruffle.rs";

let commitHash = "unknown";

try {
    const command = new Deno.Command("git", {
        args: ["rev-parse", "HEAD"],
    });
    const { stdout, success } = command.outputSync();
    if (success) {
        commitHash = new TextDecoder().decode(stdout).trim();
    }
} catch {
    console.log("Couldn't fetch latest git commit...");
}

let versionName;
if (versionChannel === "stable" || versionNumber?.includes(versionChannel)) {
    versionName = versionNumber;
} else {
    versionName = `${versionChannel} ${versionNumber}`;
}

interface VersionInformation {
    version_number: string;
    version_name: string;
    version_channel: string;
    build_date: string;
    commitHash: string;
    version4: string;
    firefox_extension_id: string;
}

let versionSeal: VersionInformation;

if (Deno.env.get("ENABLE_VERSION_SEAL") === "true") {
    const sealFile = join(import.meta.dirname!, "../../../version_seal.json");
    try {
        const content = Deno.readTextFileSync(sealFile);
        console.log("Using version seal");
        // Using the version seal stored previously.
        versionSeal = JSON.parse(content) as VersionInformation;

        versionNumber = versionSeal.version_number;
        versionName = versionSeal.version_name;
        versionChannel = versionSeal.version_channel;
        buildDate = versionSeal.build_date;
        commitHash = versionSeal.commitHash;
    } catch {
        console.log("Creating version seal");
        versionSeal = {
            version_number: versionNumber,
            version_name: versionName,
            version_channel: versionChannel,
            build_date: buildDate,
            commitHash: commitHash,
            version4: Deno.env.get("VERSION4") ?? "",
            firefox_extension_id: firefoxExtensionId,
        };

        Deno.writeTextFileSync(sealFile, JSON.stringify(versionSeal));
    }
}

const fallbackWasmName =
    Deno.env.get("BUILD_WASM_MVP") === "true"
        ? "ruffle_web-wasm_mvp"
        : "ruffle_web";

const options = {
    files: "dist/**",
    from: [
        /%VERSION_NUMBER%/g,
        /%VERSION_NAME%/g,
        /%VERSION_CHANNEL%/g,
        /%BUILD_DATE%/g,
        /%COMMIT_HASH%/g,
        /%FALLBACK_WASM%/g,
    ],
    to: [
        versionNumber,
        versionName,
        versionChannel,
        buildDate,
        commitHash,
        fallbackWasmName,
    ],
};

replaceInFileSync(options);
