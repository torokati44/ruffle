import { assertEquals } from "jsr:@std/assert@1";
import { parseColor, parseDuration } from "../src/internal/builder.ts";

Deno.test("Color parsing - should parse a valid RRGGBB hex, with hash", () => {
    assertEquals(parseColor("#A1B2C3"), 0xa1b2c3);
});

Deno.test("Color parsing - should parse a valid RRGGBB hex, without hash", () => {
    assertEquals(parseColor("1A2B3C"), 0x1a2b3c);
});

Deno.test("Color parsing - should fail with not enough digits", () => {
    assertEquals(parseColor("123"), undefined);
});

Deno.test("Color parsing - should treat invalid hex as 0", () => {
    assertEquals(parseColor("#AX2Y3Z"), 0xa02030);
});

Deno.test("Duration parsing - should accept number of seconds as number", () => {
    assertEquals(parseDuration(12.3), 12.3);
});

Deno.test("Duration parsing - should accept a legacy style duration", () => {
    assertEquals(parseDuration({ secs: 12.3, nanos: 400000 }), 12.3);
});
