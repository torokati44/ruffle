import { Ruffle } from "../pkg/ruffle_web";

import { loadRuffle } from "./load-ruffle";
import { ruffleShadowTemplate } from "./shadow-template";
import { lookupElement } from "./register-element";
import { Config } from "./config";
import {
    BaseLoadOptions,
    DataLoadOptions,
    URLLoadOptions,
    AutoPlay,
    UnmuteOverlay,
} from "./load-options";
import { MovieMetadata } from "./movie-metadata";
import { InternalContextMenuItem } from "./context-menu";

export const FLASH_MIMETYPE = "application/x-shockwave-flash";
export const FUTURESPLASH_MIMETYPE = "application/futuresplash";
export const FLASH7_AND_8_MIMETYPE = "application/x-shockwave-flash2-preview";
export const FLASH_MOVIE_MIMETYPE = "application/vnd.adobe.flash-movie";
export const FLASH_ACTIVEX_CLASSID =
    "clsid:D27CDB6E-AE6D-11cf-96B8-444553540000";

const RUFFLE_ORIGIN = "https://ruffle.rs";
const DIMENSION_REGEX = /^\s*(\d+(\.\d+)?(%)?)/;

enum PanicError {
    Unknown,
    CSPConflict,
    FileProtocol,
    InvalidWasm,
    JavascriptConfiguration,
    JavascriptConflict,
    WasmCors,
    WasmMimeType,
    WasmNotFound,
}

// Safari still requires prefixed fullscreen APIs, see:
// https://developer.mozilla.org/en-US/docs/Web/API/Element/requestFullScreen
// Safari uses alternate capitalization of FullScreen in some older APIs.
declare global {
    interface Document {
        webkitFullscreenEnabled?: boolean;
        webkitFullscreenElement?: boolean;
        webkitExitFullscreen?: () => void;
        webkitCancelFullScreen?: () => void;
    }
    interface HTMLElement {
        webkitRequestFullscreen?: (arg0: unknown) => unknown;
        webkitRequestFullScreen?: (arg0: unknown) => unknown;
    }
}

/**
 * An item to show in Ruffle's custom context menu
 */
interface ContextMenuItem {
    /**
     * The text to show to the user
     */
    text: string;

    /**
     * The function to call when clicked
     *
     * @param event The mouse event that triggered the click
     */
    onClick: (event: MouseEvent) => void;

    /**
     * Whether the item is clickable
     *
     * @default true
     */
    enabled?: boolean;
}

/**
 * Converts arbitrary input to an easy to use record object.
 *
 * @param parameters Parameters to sanitize
 * @returns A sanitized map of param name to param value
 */
function sanitizeParameters(
    parameters:
        | (URLSearchParams | string | Record<string, string>)
        | undefined
        | null
): Record<string, string> {
    if (parameters === null || parameters === undefined) {
        return {};
    }
    if (!(parameters instanceof URLSearchParams)) {
        parameters = new URLSearchParams(parameters);
    }
    const output: Record<string, string> = {};

    for (const [key, value] of parameters) {
        // Every value must be type of string
        output[key] = value.toString();
    }

    return output;
}

/**
 * The ruffle player element that should be inserted onto the page.
 *
 * This element will represent the rendered and intractable flash movie.
 */
export class RufflePlayer extends HTMLElement {
    private shadow: ShadowRoot;
    private dynamicStyles: HTMLStyleElement;
    private container: HTMLElement;
    private playButton: HTMLElement;
    private unmuteOverlay: HTMLElement;

    // Firefox has a read-only "contextMenu" property,
    // so avoid shadowing it.
    private contextMenuElement: HTMLElement;
    private hasContextMenu = false;

    // Whether this device is a touch device.
    // Set to true when a touch event is encountered.
    private isTouch = false;

    private swfUrl?: string;
    private instance: Ruffle | null;
    private options: BaseLoadOptions | null;
    private _trace_observer: ((message: string) => void) | null;
    private lastActivePlayingState: boolean;

    private _metadata: MovieMetadata | null;
    private _readyState: ReadyState;

    private ruffleConstructor: Promise<typeof Ruffle>;
    private panicked = false;

    /**
     * Triggered when a movie metadata has been loaded (such as movie width and height).
     *
     * @event RufflePlayer#loadedmetadata
     */
    static LOADED_METADATA = "loadedmetadata";

    /**
     * A movie can communicate with the hosting page using fscommand
     * as long as script access is allowed.
     *
     * @param command A string passed to the host application for any use.
     * @param args A string passed to the host application for any use.
     * @returns True if the command was handled.
     */
    onFSCommand: ((command: string, args: string) => boolean) | null;

    /**
     * Any configuration that should apply to this specific player.
     * This will be defaulted with any global configuration.
     */
    config: Config = {};

    /**
     * Indicates the readiness of the playing movie.
     *
     * @returns The `ReadyState` of the player.
     */
    get readyState(): ReadyState {
        return this._readyState;
    }

    /**
     * The metadata of the playing movie (such as movie width and height).
     * These are inherent properties stored in the SWF file and are not affected by runtime changes.
     * For example, `metadata.width` is the width of the SWF file, and not the width of the Ruffle player.
     *
     * @returns The metadata of the movie, or `null` if the movie metadata has not yet loaded.
     */
    get metadata(): MovieMetadata | null {
        return this._metadata;
    }

    /**
     * Constructs a new Ruffle flash player for insertion onto the page.
     */
    constructor() {
        super();

        this.shadow = this.attachShadow({ mode: "open" });
        this.shadow.appendChild(ruffleShadowTemplate.content.cloneNode(true));

        this.dynamicStyles = <HTMLStyleElement>(
            this.shadow.getElementById("dynamic_styles")
        );
        this.container = this.shadow.getElementById("container")!;
        this.playButton = this.shadow.getElementById("play_button")!;
        if (this.playButton) {
            this.playButton.addEventListener(
                "click",
                this.playButtonClicked.bind(this)
            );
        }

        this.unmuteOverlay = this.shadow.getElementById("unmute_overlay")!;

        this.contextMenuElement = this.shadow.getElementById("context-menu")!;
        this.addEventListener("contextmenu", this.showContextMenu.bind(this));
        this.addEventListener("pointerdown", this.pointerDown.bind(this));
        window.addEventListener("click", this.hideContextMenu.bind(this));

        this.instance = null;
        this.options = null;
        this.onFSCommand = null;
        this._trace_observer = null;

        this._readyState = ReadyState.HaveNothing;
        this._metadata = null;

        this.ruffleConstructor = loadRuffle();

        this.lastActivePlayingState = false;
        this.setupPauseOnTabHidden();

        return this;
    }

    /**
     * Setup event listener to detect when tab is not active to pause instance playback.
     * this.instance.play() is called when the tab becomes visible only if the
     * the instance was not paused before tab became hidden.
     *
     * See:
     *      https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API
     * @ignore
     * @internal
     */
    setupPauseOnTabHidden(): void {
        document.addEventListener(
            "visibilitychange",
            () => {
                if (!this.instance) return;

                // Tab just changed to be inactive. Record whether instance was playing.
                if (document.hidden) {
                    this.lastActivePlayingState = this.instance.is_playing();
                    this.instance.pause();
                }
                // Play only if instance was playing originally.
                if (!document.hidden && this.lastActivePlayingState === true) {
                    this.instance.play();
                }
            },
            false
        );
    }

    /**
     * @ignore
     * @internal
     */
    connectedCallback(): void {
        this.updateStyles();
    }

    /**
     * @ignore
     * @internal
     */
    static get observedAttributes(): string[] {
        return ["width", "height"];
    }

    /**
     * @ignore
     * @internal
     */
    attributeChangedCallback(
        name: string,
        _oldValue: string | undefined,
        _newValue: string | undefined
    ): void {
        if (name === "width" || name === "height") {
            this.updateStyles();
        }
    }

    /**
     * @ignore
     * @internal
     */
    disconnectedCallback(): void {
        this.destroy();
    }

    /**
     * Updates the internal shadow DOM to reflect any set attributes from
     * this element.
     *
     * @protected
     */
    protected updateStyles(): void {
        if (this.dynamicStyles.sheet) {
            if (this.dynamicStyles.sheet.rules) {
                for (
                    let i = 0;
                    i < this.dynamicStyles.sheet.rules.length;
                    i++
                ) {
                    this.dynamicStyles.sheet.deleteRule(i);
                }
            }

            const widthAttr = this.attributes.getNamedItem("width");
            if (widthAttr !== undefined && widthAttr !== null) {
                const width = RufflePlayer.htmlDimensionToCssDimension(
                    widthAttr.value
                );
                if (width !== null) {
                    this.dynamicStyles.sheet.insertRule(
                        `:host { width: ${width}; }`
                    );
                }
            }

            const heightAttr = this.attributes.getNamedItem("height");
            if (heightAttr !== undefined && heightAttr !== null) {
                const height = RufflePlayer.htmlDimensionToCssDimension(
                    heightAttr.value
                );
                if (height !== null) {
                    this.dynamicStyles.sheet.insertRule(
                        `:host { height: ${height}; }`
                    );
                }
            }
        }
    }

    /**
     * Determine if this element is the fallback content of another Ruffle
     * player.
     *
     * This heuristic assumes Ruffle objects will never use their fallback
     * content. If this changes, then this code also needs to change.
     *
     * @private
     */
    private isUnusedFallbackObject(): boolean {
        let parent = this.parentNode;
        const element = lookupElement("ruffle-object");

        if (element !== null) {
            while (parent != document && parent != null) {
                if (parent.nodeName === element.name) {
                    return true;
                }

                parent = parent.parentNode;
            }
        }

        return false;
    }

    /**
     * Ensure a fresh Ruffle instance is ready on this player before continuing.
     *
     * @throws Any exceptions generated by loading Ruffle Core will be logged
     * and passed on.
     *
     * @private
     */
    private async ensureFreshInstance(config: BaseLoadOptions): Promise<void> {
        this.destroy();

        const ruffleConstructor = await this.ruffleConstructor.catch((e) => {
            console.error(`Serious error loading Ruffle: ${e}`);

            // Serious duck typing. In error conditions, let's not make assumptions.
            if (window.location.protocol === "file:") {
                e.ruffleIndexError = PanicError.FileProtocol;
            } else {
                e.ruffleIndexError = PanicError.WasmNotFound;
                const message = String(e.message).toLowerCase();
                if (message.includes("mime")) {
                    e.ruffleIndexError = PanicError.WasmMimeType;
                } else if (
                    message.includes("networkerror") ||
                    message.includes("failed to fetch")
                ) {
                    e.ruffleIndexError = PanicError.WasmCors;
                } else if (message.includes("disallowed by embedder")) {
                    e.ruffleIndexError = PanicError.CSPConflict;
                } else if (
                    message.includes("webassembly.instantiate") &&
                    e.name === "CompileError"
                ) {
                    e.ruffleIndexError = PanicError.InvalidWasm;
                } else if (
                    !message.includes("magic") &&
                    (e.name === "CompileError" || e.name === "TypeError")
                ) {
                    e.ruffleIndexError = PanicError.JavascriptConflict;
                }
            }
            this.panic(e);
            throw e;
        });

        this.instance = new ruffleConstructor(this.container, this, config);
        console.log("New Ruffle instance created.");

        // In Firefox, AudioContext.state is always "suspended" when the object has just been created.
        // It may change by itself to "running" some milliseconds later. So we need to wait a little
        // bit before checking if autoplay is supported and applying the instance config.
        if (this.audioState() !== "running") {
            this.container.style.visibility = "hidden";
            await new Promise<void>((resolve) => {
                window.setTimeout(() => {
                    resolve();
                }, 200);
            });
            this.container.style.visibility = "";
        }

        const autoplay = Object.values(Object(AutoPlay)).includes(
            config.autoplay
        )
            ? config.autoplay
            : AutoPlay.Auto;
        const unmuteVisibility = Object.values(Object(UnmuteOverlay)).includes(
            config.unmuteOverlay
        )
            ? config.unmuteOverlay
            : UnmuteOverlay.Visible;

        if (
            autoplay == AutoPlay.On ||
            (autoplay == AutoPlay.Auto && this.audioState() === "running")
        ) {
            this.play();

            if (this.audioState() !== "running") {
                if (unmuteVisibility === UnmuteOverlay.Visible) {
                    this.unmuteOverlay.style.display = "block";
                }

                this.container.addEventListener(
                    "click",
                    this.unmuteOverlayClicked.bind(this),
                    {
                        once: true,
                    }
                );

                const audioContext = this.instance?.audio_context();
                if (audioContext) {
                    audioContext.onstatechange = () => {
                        if (audioContext.state === "running") {
                            this.unmuteOverlayClicked();
                        }
                        audioContext.onstatechange = null;
                    };
                }
            }
        } else {
            this.playButton.style.display = "block";
        }
    }

    /**
     * Destroys the currently running instance of Ruffle.
     */
    private destroy(): void {
        if (this.instance) {
            this.instance.destroy();
            this.instance = null;
            this._metadata = null;
            this._readyState = ReadyState.HaveNothing;
            console.log("Ruffle instance destroyed.");
        }
    }

    /**
     * Loads a specified movie into this player.
     *
     * This will replace any existing movie that may be playing.
     *
     * @param options One of the following:
     * - A URL, passed as a string, which will load a URL with default options.
     * - A [[URLLoadOptions]] object, to load a URL with options.
     * - A [[DataLoadOptions]] object, to load data with options.
     *
     * The options will be defaulted by the [[config]] field, which itself
     * is defaulted by a global `window.RufflePlayer.config`.
     */
    async load(
        options: string | URLLoadOptions | DataLoadOptions
    ): Promise<void> {
        let optionsError = "";
        switch (typeof options) {
            case "string":
                options = { url: options };
                break;
            case "object":
                if (options === null) {
                    optionsError = "Argument 0 must be a string or object";
                } else if (!("url" in options) && !("data" in options)) {
                    optionsError =
                        "Argument 0 must contain a `url` or `data` key";
                } else if (
                    "url" in options &&
                    typeof options.url !== "string"
                ) {
                    optionsError = "`url` must be a string";
                }
                break;
            default:
                optionsError = "Argument 0 must be a string or object";
                break;
        }
        if (optionsError.length > 0) {
            const error = new TypeError(optionsError);
            error.ruffleIndexError = PanicError.JavascriptConfiguration;
            this.panic(error);
            throw error;
        }

        if (!this.isConnected || this.isUnusedFallbackObject()) {
            console.warn(
                "Ignoring attempt to play a disconnected or suspended Ruffle element"
            );
            return;
        }

        try {
            const config: BaseLoadOptions = {
                ...(window.RufflePlayer?.config ?? {}),
                ...this.config,
                ...options,
            };
            // `allowScriptAccess` can only be set in `options`.
            config.allowScriptAccess = options.allowScriptAccess;

            this.options = options;
            this.hasContextMenu = config.contextMenu !== false;

            // Pre-emptively set background color of container while Ruffle/SWF loads.
            if (config.backgroundColor) {
                this.container.style.backgroundColor = config.backgroundColor;
            }

            await this.ensureFreshInstance(config);

            if ("url" in options) {
                console.log(`Loading SWF file ${options.url}`);
                try {
                    this.swfUrl = new URL(
                        options.url,
                        document.location.href
                    ).href;
                } catch {
                    this.swfUrl = options.url;
                }

                const parameters = {
                    ...sanitizeParameters(
                        options.url.substring(options.url.indexOf("?"))
                    ),
                    ...sanitizeParameters(options.parameters),
                };

                this.instance!.stream_from(options.url, parameters);
            } else if ("data" in options) {
                console.log("Loading SWF data");
                this.instance!.load_data(
                    new Uint8Array(options.data),
                    sanitizeParameters(options.parameters)
                );
            }
        } catch (err) {
            console.error(`Serious error occurred loading SWF file: ${err}`);
            throw err;
        }
    }

    private playButtonClicked(): void {
        this.play();
    }

    /**
     * Plays or resumes the movie.
     */
    play(): void {
        if (this.instance) {
            this.instance.play();
            if (this.playButton) {
                this.playButton.style.display = "none";
            }
        }
    }

    /**
     * Checks if this player is allowed to be fullscreen by the browser.
     *
     * @returns True if you may call [[enterFullscreen]].
     */
    get fullscreenEnabled(): boolean {
        return !!(
            document.fullscreenEnabled || document.webkitFullscreenEnabled
        );
    }

    /**
     * Checks if this player is currently fullscreen inside the browser.
     *
     * @returns True if it is fullscreen.
     */
    get isFullscreen(): boolean {
        return (
            (document.fullscreenElement || document.webkitFullscreenElement) ===
            this
        );
    }

    /**
     * Requests the browser to make this player fullscreen.
     *
     * This is not guaranteed to succeed, please check [[fullscreenEnabled]] first.
     */
    enterFullscreen(): void {
        const options = {
            navigationUI: "hide",
        } as const;
        if (this.requestFullscreen) {
            this.requestFullscreen(options);
        } else if (this.webkitRequestFullscreen) {
            this.webkitRequestFullscreen(options);
        } else if (this.webkitRequestFullScreen) {
            this.webkitRequestFullScreen(options);
        }
    }

    /**
     * Requests the browser to no longer make this player fullscreen.
     */
    exitFullscreen(): void {
        if (document.exitFullscreen) {
            document.exitFullscreen();
        } else if (document.webkitExitFullscreen) {
            document.webkitExitFullscreen();
        } else if (document.webkitCancelFullScreen) {
            document.webkitCancelFullScreen();
        }
    }

    private pointerDown(event: PointerEvent): void {
        // Disable context menu when touch support is being used
        // to avoid a long press triggering the context menu. (#1972)
        if (event.pointerType === "touch" || event.pointerType === "pen") {
            this.isTouch = true;
        }
    }

    private contextMenuItems(): Array<ContextMenuItem | null> {
        const CHECKMARK = String.fromCharCode(0x2713);
        const items = [];

        if (this.instance) {
            const customItems: InternalContextMenuItem[] = this.instance.prepare_context_menu();
            customItems.forEach((item, index) => {
                if (item.separatorBefore) items.push(null);
                items.push({
                    // TODO: better checkboxes
                    text:
                        item.caption + (item.checked ? ` (${CHECKMARK})` : ``),
                    onClick: () =>
                        this.instance?.run_context_menu_callback(index),
                    enabled: item.enabled,
                });
            });
        }
        items.push(null);

        if (this.fullscreenEnabled) {
            if (this.isFullscreen) {
                items.push({
                    text: "Exit fullscreen",
                    onClick: this.exitFullscreen.bind(this),
                });
            } else {
                items.push({
                    text: "Enter fullscreen",
                    onClick: this.enterFullscreen.bind(this),
                });
            }
        }
        items.push(null);
        items.push({
            text: `About Ruffle (z0r special edition)`,
            onClick() {
                window.open(RUFFLE_ORIGIN, "_blank");
            },
        });
        return items;
    }

    private showContextMenu(e: MouseEvent): void {
        e.preventDefault();

        if (!this.hasContextMenu || this.isTouch) {
            return;
        }

        // Clear all context menu items.
        while (this.contextMenuElement.firstChild) {
            this.contextMenuElement.removeChild(
                this.contextMenuElement.firstChild
            );
        }

        // Populate context menu items.
        for (const item of this.contextMenuItems()) {
            if (item === null) {
                if (!this.contextMenuElement.lastElementChild) continue; // don't start with separators
                if (
                    this.contextMenuElement.lastElementChild.classList.contains(
                        "menu_separator"
                    )
                )
                    continue; // don't repeat separators

                const menuSeparator = document.createElement("li");
                menuSeparator.className = "menu_separator";
                const hr = document.createElement("hr");
                menuSeparator.appendChild(hr);
                this.contextMenuElement.appendChild(menuSeparator);
            } else {
                const { text, onClick, enabled } = item;
                const menuItem = document.createElement("li");
                menuItem.className = "menu_item active";
                menuItem.textContent = text;
                this.contextMenuElement.appendChild(menuItem);

                if (enabled !== false) {
                    menuItem.addEventListener("click", onClick);
                } else {
                    menuItem.classList.add("disabled");
                }
            }
        }

        // Place a context menu in the top-left corner, so
        // its `clientWidth` and `clientHeight` are not clamped.
        this.contextMenuElement.style.left = "0";
        this.contextMenuElement.style.top = "0";
        this.contextMenuElement.style.display = "block";

        const rect = this.getBoundingClientRect();
        const x = e.clientX - rect.x;
        const y = e.clientY - rect.y;
        const maxX = rect.width - this.contextMenuElement.clientWidth - 1;
        const maxY = rect.height - this.contextMenuElement.clientHeight - 1;

        this.contextMenuElement.style.left =
            Math.floor(Math.min(x, maxX)) + "px";
        this.contextMenuElement.style.top =
            Math.floor(Math.min(y, maxY)) + "px";
    }

    private hideContextMenu(): void {
        this.instance?.clear_custom_menu_items();
        this.contextMenuElement.style.display = "none";
    }

    /**
     * Pauses this player.
     *
     * No more frames, scripts or sounds will be executed.
     * This movie will be considered inactive and will not wake up until resumed.
     */
    pause(): void {
        if (this.instance) {
            this.instance.pause();
            if (this.playButton) {
                this.playButton.style.display = "block";
            }
        }
    }

    private audioState(): string {
        if (this.instance) {
            const audioContext = this.instance.audio_context();
            return (audioContext && audioContext.state) || "running";
        }
        return "suspended";
    }

    private unmuteOverlayClicked(): void {
        if (this.instance) {
            if (this.audioState() !== "running") {
                const audioContext = this.instance.audio_context();
                if (audioContext) {
                    audioContext.resume();
                }
            }
            if (this.unmuteOverlay) {
                this.unmuteOverlay.style.display = "none";
            }
        }
    }

    /**
     * Copies attributes and children from another element to this player element.
     * Used by the polyfill elements, RuffleObject and RuffleEmbed.
     *
     * @param elem The element to copy all attributes from.
     *
     * @protected
     */
    protected copyElement(elem: HTMLElement): void {
        if (elem) {
            for (let i = 0; i < elem.attributes.length; i++) {
                const attrib = elem.attributes[i];
                if (attrib.specified) {
                    // Issue 468: Chrome "Click to Active Flash" box stomps on title attribute
                    if (
                        attrib.name === "title" &&
                        attrib.value === "Adobe Flash Player"
                    ) {
                        continue;
                    }

                    try {
                        this.setAttribute(attrib.name, attrib.value);
                    } catch (err) {
                        // The embed may have invalid attributes, so handle these gracefully.
                        console.warn(
                            `Unable to set attribute ${attrib.name} on Ruffle instance`
                        );
                    }
                }
            }

            for (const node of Array.from(elem.children)) {
                this.appendChild(node);
            }
        }
    }

    /**
     * Converts a dimension attribute on an HTML embed/object element to a valid CSS dimension.
     * HTML element dimensions are unitless, but can also be percentages.
     * Add a 'px' unit unless the value is a percentage.
     * Returns null if this is not a valid dimension.
     *
     * @param attribute The attribute to convert
     *
     * @private
     */
    private static htmlDimensionToCssDimension(
        attribute: string
    ): string | null {
        if (attribute) {
            const match = attribute.match(DIMENSION_REGEX);
            if (match) {
                let out = match[1];
                if (!match[3]) {
                    // Unitless -- add px for CSS.
                    out += "px";
                }
                return out;
            }
        }
        return null;
    }

    /**
     * When a movie presents a new callback through `ExternalInterface.addCallback`,
     * we are informed so that we can expose the method on any relevant DOM element.
     *
     * This should only be called by Ruffle itself and not by users.
     *
     * @param name The name of the callback that is now available.
     *
     * @internal
     * @ignore
     */
    onCallbackAvailable(name: string): void {
        const instance = this.instance;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (<any>this)[name] = (...args: any[]) => {
            return instance?.call_exposed_callback(name, args);
        };
    }

    /**
     * Sets a trace observer on this flash player.
     *
     * The observer will be called, as a function, for each message that the playing movie will "trace" (output).
     *
     * @param observer The observer that will be called for each trace.
     */
    set traceObserver(observer: ((message: string) => void) | null) {
        this.instance?.set_trace_observer(observer);
    }

    /**
     * Panics this specific player, forcefully destroying all resources and displays an error message to the user.
     *
     * This should be called when something went absolutely, incredibly and disastrously wrong and there is no chance
     * of recovery.
     *
     * Ruffle will attempt to isolate all damage to this specific player instance, but no guarantees can be made if there
     * was a core issue which triggered the panic. If Ruffle is unable to isolate the cause to a specific player, then
     * all players will panic and Ruffle will become "poisoned" - no more players will run on this page until it is
     * reloaded fresh.
     *
     * @param error The error, if any, that triggered this panic.
     */
    panic(error: Error | null): void {
        if (this.panicked) {
            // Only show the first major error, not any repeats - they aren't as important
            return;
        }
        this.panicked = true;

        if (
            error instanceof Error &&
            (error.name === "AbortError" ||
                error.message.includes("AbortError"))
        ) {
            // Firefox: Don't display the panic screen if the user leaves the page while something is still loading
            return;
        }

        const errorIndex = error?.ruffleIndexError ?? PanicError.Unknown;

        const errorArray: Array<string | null> & {
            stackIndex: number;
        } = Object.assign([], {
            stackIndex: -1,
        });

        errorArray.push("# Error Info\n");

        if (error instanceof Error) {
            errorArray.push(`Error name: ${error.name}\n`);
            errorArray.push(`Error message: ${error.message}\n`);
            if (error.stack) {
                const stackIndex =
                    errorArray.push(
                        `Error stack:\n\`\`\`\n${error.stack}\n\`\`\`\n`
                    ) - 1;
                errorArray.stackIndex = stackIndex;
            }
        } else {
            errorArray.push(`Error: ${error}\n`);
        }

        errorArray.push("\n# Player Info\n");
        errorArray.push(this.debugPlayerInfo());

        errorArray.push("\n# Page Info\n");
        errorArray.push(`Page URL: ${document.location.href}\n`);
        if (this.swfUrl) errorArray.push(`SWF URL: ${this.swfUrl}\n`);

        errorArray.push("\n# Browser Info\n");
        errorArray.push(`Useragent: ${window.navigator.userAgent}\n`);
        errorArray.push(`OS: ${window.navigator.platform}\n`);

        errorArray.push("\n# Ruffle Info\n");
        errorArray.push(`Version: %VERSION_NUMBER%\n`);
        errorArray.push(`Name: %VERSION_NAME%\n`);
        errorArray.push(`Channel: %VERSION_CHANNEL%\n`);
        errorArray.push(`Built: %BUILD_DATE%\n`);
        errorArray.push(`Commit: %COMMIT_HASH%\n`);

        const errorText = errorArray.join("");

        // Remove query params for the issue title.
        const pageUrl = document.location.href.split(/[?#]/)[0];
        const issueTitle = `Error on ${pageUrl}`;
        let issueLink = `https://github.com/ruffle-rs/ruffle/issues/new?title=${encodeURIComponent(
            issueTitle
        )}&body=`;
        let issueBody = encodeURIComponent(errorText);
        if (
            errorArray.stackIndex > -1 &&
            String(issueLink + issueBody).length > 8195
        ) {
            // Strip the stack error from the array when the produced URL is way too long.
            // This should prevent "414 Request-URI Too Large" errors on Github.
            errorArray[errorArray.stackIndex] = null;
            issueBody = encodeURIComponent(errorArray.join(""));
        }
        issueLink += issueBody;

        // Clears out any existing content (ie play button or canvas) and replaces it with the error screen
        let errorBody, errorFooter;
        switch (errorIndex) {
            case PanicError.FileProtocol:
                // General error: Running on the `file:` protocol
                errorBody = `
                    <p>It appears you are running Ruffle on the "file:" protocol.</p>
                    <p>This doesn't work as browsers block many features from working for security reasons.</p>
                    <p>Instead, we invite you to setup a local server or either use the web demo or the desktop application.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="${RUFFLE_ORIGIN}/demo">Web Demo</a></li>
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/tags">Desktop Application</a></li>
                `;
                break;
            case PanicError.JavascriptConfiguration:
                // General error: Incorrect JavaScript configuration
                errorBody = `
                    <p>Ruffle has encountered a major issue due to an incorrect JavaScript configuration.</p>
                    <p>If you are the server administrator, we invite you to check the error details to find out which parameter is at fault.</p>
                    <p>You can also consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#javascript-api">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.WasmNotFound:
                // Self hosted: Cannot load `.wasm` file - file not found
                errorBody = `
                    <p>Ruffle failed to load the required ".wasm" file component.</p>
                    <p>If you are the server administrator, please ensure the file has correctly been uploaded.</p>
                    <p>If the issue persists, you may need to use the "publicPath" setting: please consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#configuration-options">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.WasmMimeType:
                // Self hosted: Cannot load `.wasm` file - incorrect MIME type
                errorBody = `
                    <p>Ruffle has encountered a major issue whilst trying to initialize.</p>
                    <p>This web server is not serving ".wasm" files with the correct MIME type.</p>
                    <p>If you are the server administrator, please consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#configure-webassembly-mime-type">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.WasmCors:
                // Self hosted: Cannot load `.wasm` file - CORS issues
                errorBody = `
                    <p>Ruffle failed to load the required ".wasm" file component.</p>
                    <p>Access to fetch has likely been blocked by CORS policy.</p>
                    <p>If you are the server administrator, please consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#web">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.InvalidWasm:
                // Self hosted: Cannot load `.wasm` file - incorrect configuration or missing files
                errorBody = `
                    <p>Ruffle has encountered a major issue whilst trying to initialize.</p>
                    <p>It seems like this page has missing or invalid files for running Ruffle.</p>
                    <p>If you are the server administrator, please consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#addressing-a-compileerror">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.JavascriptConflict:
                // Self hosted: Cannot load `.wasm` file - a native object / function is overriden
                errorBody = `
                    <p>Ruffle has encountered a major issue whilst trying to initialize.</p>
                    <p>It seems like this page uses JavaScript code that conflicts with Ruffle.</p>
                    <p>If you are the server administrator, we invite you to try loading the file on a blank page.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="${issueLink}">Report Bug</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            case PanicError.CSPConflict:
                // General error: Cannot load `.wasm` file - a native object / function is overriden
                errorBody = `
                    <p>Ruffle has encountered a major issue whilst trying to initialize.</p>
                    <p>This web server's Content Security Policy does not allow the required ".wasm" component to run.</p>
                    <p>If you are the server administrator, please consult the Ruffle wiki for help.</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="https://github.com/ruffle-rs/ruffle/wiki/Using-Ruffle#configure-wasm-csp">View Ruffle Wiki</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
            default:
                // Unknown error
                errorBody = `
                    <p>Ruffle has encountered a major issue whilst trying to display this Flash content.</p>
                    <p>This isn't supposed to happen, so we'd really appreciate if you could file a bug!</p>
                `;
                errorFooter = `
                    <li><a target="_top" href="${issueLink}">Report Bug</a></li>
                    <li><a href="#" id="panic-view-details">View Error Details</a></li>
                `;
                break;
        }
        this.container.innerHTML = `
            <div id="panic">
                <div id="panic-title">Something went wrong :(</div>
                <div id="panic-body">${errorBody}</div>
                <div id="panic-footer">
                    <ul>${errorFooter}</ul>
                </div>
            </div>
        `;
        const viewDetails = <HTMLLinkElement>(
            this.container.querySelector("#panic-view-details")
        );
        if (viewDetails) {
            viewDetails.onclick = () => {
                const panicBody = <HTMLDivElement>(
                    this.container.querySelector("#panic-body")
                );
                panicBody.classList.add("details");
                panicBody.innerHTML = `<textarea>${errorText}</textarea>`;
                return false;
            };
        }

        // Do this last, just in case it causes any cascading issues.
        this.destroy();
    }

    displayUnsupportedMessage(): void {
        const div = document.createElement("div");
        div.id = "message_overlay";
        // TODO: Change link to https://ruffle.rs/faq or similar
        // TODO: Pause content until message is dismissed
        div.innerHTML = `<div class="message">
            <p>Flash Player has been removed from browsers in 2021.</p>
            <p>This content is not yet supported by the Ruffle emulator and will likely not run as intended.</p>
            <div>
                <a target="_top" class="more-info-link" href="https://github.com/ruffle-rs/ruffle/wiki/Frequently-Asked-Questions-For-Users">More info</a>
                <button id="run-anyway-btn">Run anyway</button>
            </div>
        </div>`;
        this.container.prepend(div);
        const button = <HTMLButtonElement>div.querySelector("#run-anyway-btn");
        button.onclick = () => {
            div.parentNode!.removeChild(div);
        };
    }

    displayMessage(message: string): void {
        // Show a dismissible message in front of the player
        const div = document.createElement("div");
        div.id = "message_overlay";
        div.innerHTML = `<div class="message">
            <p>${message}</p>
            <div>
                <button id="continue-btn">continue</button>
            </div>
        </div>`;
        this.container.prepend(div);
        (<HTMLButtonElement>(
            this.container.querySelector("#continue-btn")
        )).onclick = () => {
            div.parentNode!.removeChild(div);
        };
    }

    protected debugPlayerInfo(): string {
        return `Allows script access: ${
            this.options?.allowScriptAccess ?? false
        }\n`;
    }

    private setMetadata(metadata: MovieMetadata) {
        this._metadata = metadata;
        // TODO: Switch this to ReadyState.Loading when we have streaming support.
        this._readyState = ReadyState.Loaded;
        this.dispatchEvent(new Event(RufflePlayer.LOADED_METADATA));
    }
}

/**
 * Describes the loading state of an SWF movie.
 */
export enum ReadyState {
    /**
     * No movie is loaded, or no information is yet available about the movie.
     */
    HaveNothing = 0,

    /**
     * The movie is still loading, but it has started playback, and metadata is available.
     */
    Loading = 1,

    /**
     * The movie has completely loaded.
     */
    Loaded = 2,
}

/**
 * Returns whether a SWF file can call JavaScript code in the surrounding HTML file.
 *
 * @param access The value of the `allowScriptAccess` attribute.
 * @param url The URL of the SWF file.
 * @returns True if script access is allowed.
 */
export function isScriptAccessAllowed(
    access: string | null,
    url: string
): boolean {
    if (!access) {
        access = "sameDomain";
    }
    switch (access.toLowerCase()) {
        case "always":
            return true;
        case "never":
            return false;
        case "samedomain":
        default:
            try {
                return (
                    new URL(window.location.href).origin ===
                    new URL(url, window.location.href).origin
                );
            } catch {
                return false;
            }
    }
}

/**
 * Returns whether the given filename ends in a known flash extension.
 *
 * @param filename The filename to test.
 * @returns True if the filename is a flash movie (swf or spl).
 */
export function isSwfFilename(filename: string | null): boolean {
    if (filename) {
        let pathname = "";
        try {
            // A base URL is required if `filename` is a relative URL, but we don't need to detect the real URL origin.
            pathname = new URL(filename, RUFFLE_ORIGIN).pathname;
        } catch (err) {
            // Some invalid filenames, like `///`, could raise a TypeError. Let's fail silently in this situation.
        }
        if (pathname && pathname.length >= 4) {
            const extension = pathname.slice(-4).toLowerCase();
            if (extension === ".swf" || extension === ".spl") {
                return true;
            }
        }
    }
    return false;
}
