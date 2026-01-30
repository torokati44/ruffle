import { assertEquals } from "jsr:@std/assert@1";
import { swfFileName } from "../src/swf-utils.ts";

function nameFor(url: string): string {
    return swfFileName(new URL(url));
}

Deno.test("swfFileName - should extract simple SWF name", () => {
    assertEquals(nameFor("http://example.com/file.swf"), "file.swf");
});

Deno.test("swfFileName - should not include query parameters", () => {
    assertEquals(
        nameFor(
            "https://uploads.ungrounded.net/574000/574241_DiamondNGSP.swf?123",
        ),
        "574241_DiamondNGSP.swf",
    );
});
