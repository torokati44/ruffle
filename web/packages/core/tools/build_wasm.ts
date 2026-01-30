function runWasmOpt({ path, flags }: { path: string; flags?: string[] }) {
    let args = ["-o", path, "-O", "-g", path];
    if (flags) {
        args = args.concat(flags);
    }
    const command = new Deno.Command("wasm-opt", {
        args,
        stdout: "inherit",
        stderr: "inherit",
    });
    const { success } = command.outputSync();
    if (!success) {
        throw new Error("wasm-opt failed");
    }
}
function runWasmBindgen({
    path,
    outName,
    flags,
    dir,
}: {
    path: string;
    outName: string;
    flags?: string[];
    dir: string;
}) {
    let args = [
        path,
        "--target",
        "web",
        "--out-dir",
        dir,
        "--out-name",
        outName,
    ];
    if (flags) {
        args = args.concat(flags);
    }
    const command = new Deno.Command("wasm-bindgen", {
        args,
        stdout: "inherit",
        stderr: "inherit",
    });
    const { success } = command.outputSync();
    if (!success) {
        throw new Error("wasm-bindgen failed");
    }
}
function cargoBuild({
    profile,
    features,
    rustFlags,
    extensions,
}: {
    profile?: string;
    features?: string[];
    rustFlags?: string[];
    extensions?: boolean;
}) {
    let args = ["build", "--locked", "--target", "wasm32-unknown-unknown"];
    if (!extensions) {
        args.push("-Z");
        args.push("build-std=std,panic_abort");
    }

    if (profile) {
        args.push("--profile", profile);
    }
    if (Deno.env.get("CARGO_FEATURES")) {
        features = (features || []).concat(
            Deno.env.get("CARGO_FEATURES")!.split(","),
        );
    }
    if (features) {
        args.push("--features", features.join(","));
    }
    let totalRustFlags = Deno.env.get("RUSTFLAGS") || "";
    if (rustFlags) {
        if (totalRustFlags) {
            totalRustFlags += ` ${rustFlags.join(" ")}`;
        } else {
            totalRustFlags = rustFlags.join(" ");
        }
    }
    if (Deno.env.get("CARGO_FLAGS")) {
        args = args.concat(Deno.env.get("CARGO_FLAGS")!.split(" "));
    }
    const command = new Deno.Command("cargo", {
        args,
        env: {
            ...Deno.env.toObject(),
            RUSTFLAGS: totalRustFlags,
            RUSTC_BOOTSTRAP: extensions ? "0" : "1",
        },
        stdout: "inherit",
        stderr: "inherit",
    });
    const { success } = command.outputSync();
    if (!success) {
        throw new Error("cargo build failed");
    }
}
function buildWasm(
    profile: string,
    filename: string,
    optimise: boolean,
    extensions: boolean,
    wasmSource: string,
) {
    const rustFlags = [
        "--cfg=web_sys_unstable_apis",
        '--cfg=getrandom_backend="wasm_js"',
        "-Aunknown_lints",
    ];
    const wasmBindgenFlags = [];
    const wasmOptFlags = [];
    const flavor = extensions ? "extensions" : "vanilla";
    if (extensions) {
        rustFlags.push(
            "-C",
            "target-feature=+bulk-memory,+simd128,+nontrapping-fptoint,+sign-ext,+reference-types",
        );
        wasmBindgenFlags.push("--reference-types");
        wasmOptFlags.push("--enable-reference-types");
    } else {
        rustFlags.push("-C", "target-cpu=mvp");
    }
    let originalWasmPath;
    if (wasmSource === "cargo" || wasmSource === "cargo_and_store") {
        console.log(`Building ${flavor} with cargo...`);
        cargoBuild({
            profile,
            rustFlags,
            extensions,
        });
        originalWasmPath = `../../../target/wasm32-unknown-unknown/${profile}/ruffle_web.wasm`;
        if (wasmSource === "cargo_and_store") {
            Deno.copyFileSync(originalWasmPath, `../../dist/${filename}.wasm`);
        }
    } else if (wasmSource === "existing") {
        originalWasmPath = `../../dist/${filename}.wasm`;
    } else {
        throw new Error(
            "Invalid wasm source: must be one of 'cargo', 'cargo_and_store' or 'existing'",
        );
    }
    console.log(`Running wasm-bindgen on ${flavor}...`);
    runWasmBindgen({
        path: originalWasmPath,
        outName: filename,
        dir: "dist",
        flags: wasmBindgenFlags,
    });
    if (optimise) {
        console.log(`Running wasm-opt on ${flavor}...`);
        runWasmOpt({
            path: `dist/${filename}_bg.wasm`,
            flags: wasmOptFlags,
        });
    }
}
function detectWasmOpt() {
    try {
        const command = new Deno.Command("wasm-opt", {
            args: ["--version"],
            stdout: "null",
            stderr: "null",
        });
        const { success } = command.outputSync();
        return success;
    } catch (_a) {
        return false;
    }
}
const buildWasmMvp = !!Deno.env.get("BUILD_WASM_MVP");
const wasmSource = Deno.env.get("WASM_SOURCE") || "cargo";
const hasWasmOpt = detectWasmOpt();
if (!hasWasmOpt) {
    console.log(
        "NOTE: Since wasm-opt could not be found (or it failed), the resulting module might not perform that well, but it should still work.",
    );
}
if (wasmSource === "cargo_and_store") {
    try {
        Deno.removeSync("../../dist", { recursive: true });
    } catch {
        // Directory might not exist
    }
    Deno.mkdirSync("../../dist");
}
buildWasm("web-wasm-extensions", "ruffle_web", hasWasmOpt, true, wasmSource);
if (buildWasmMvp) {
    buildWasm(
        "web-wasm-mvp",
        "ruffle_web-wasm_mvp",
        hasWasmOpt,
        false,
        wasmSource,
    );
}
