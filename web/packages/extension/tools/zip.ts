import { dirname } from "jsr:@std/path@1";
import { ensureDir } from "jsr:@std/fs@1";
import archiver from "archiver";
import { Writable } from "node:stream";

async function zip(source: string, destination: string) {
    await ensureDir(dirname(destination));

    const file = await Deno.open(destination, { write: true, create: true, truncate: true });
    const output = new Writable({
        write(chunk, encoding, callback) {
            file.write(chunk).then(() => callback()).catch(callback);
        },
        final(callback) {
            file.close().then(() => callback()).catch(callback);
        }
    });

    const archive = archiver("zip");

    output.on("close", () => {
        console.log(
            `Extension is ${archive.pointer()} total bytes when packaged.`,
        );
    });

    archive.on("error", (error) => {
        throw error;
    });

    archive.on("warning", (error) => {
        if (error.code === "ENOENT") {
            console.warn(`Warning whilst zipping extension: ${error}`);
        } else {
            throw error;
        }
    });

    archive.pipe(output);

    archive.directory(source, "");

    await archive.finalize();
}
const assets = new URL("../assets/", import.meta.url).pathname;
zip(assets, Deno.args[0] ?? "").catch(console.error);
