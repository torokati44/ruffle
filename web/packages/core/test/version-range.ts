import { assertEquals } from "jsr:@std/assert@1";
import { VersionRange } from "../src/version-range.ts";
import { Version } from "../src/version.ts";

Deno.test("VersionRange - from_requirement_string() - should accept a specific version without an equals sign", () => {
    const range = VersionRange.fromRequirementString("1.2.3");
    assertEquals(range.requirements, [
        [{ comparator: "", version: Version.fromSemver("1.2.3") }],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should accept two different versions without equals signs", () => {
    const range = VersionRange.fromRequirementString("1.2.3 || 1.2.4");
    assertEquals(range.requirements, [
        [{ comparator: "", version: Version.fromSemver("1.2.3") }],
        [{ comparator: "", version: Version.fromSemver("1.2.4") }],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should accept a specific version with an equals sign", () => {
    const range = VersionRange.fromRequirementString("=1.2.3");
    assertEquals(range.requirements, [
        [{ comparator: "=", version: Version.fromSemver("1.2.3") }],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should accept two versions with equals signs", () => {
    const range =
        VersionRange.fromRequirementString("=1.2.3 || =1.2.4");
    assertEquals(range.requirements, [
        [{ comparator: "=", version: Version.fromSemver("1.2.3") }],
        [{ comparator: "=", version: Version.fromSemver("1.2.4") }],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should accept a min and max range", () => {
    const range = VersionRange.fromRequirementString(">1.2.3 <1.2.5");
    assertEquals(range.requirements, [
        [
            { comparator: ">", version: Version.fromSemver("1.2.3") },
            { comparator: "<", version: Version.fromSemver("1.2.5") },
        ],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should allow inclusive range", () => {
    const range =
        VersionRange.fromRequirementString(">=1-test <=2-test");
    assertEquals(range.requirements, [
        [
            {
                comparator: ">=",
                version: Version.fromSemver("1-test"),
            },
            {
                comparator: "<=",
                version: Version.fromSemver("2-test"),
            },
        ],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should ignore extra whitespace within a range", () => {
    const range = VersionRange.fromRequirementString("^1.2   <1.3");
    assertEquals(range.requirements, [
        [
            { comparator: "^", version: Version.fromSemver("1.2") },
            { comparator: "<", version: Version.fromSemver("1.3") },
        ],
    ]);
});

Deno.test("VersionRange - from_requirement_string() - should ignore empty ranges", () => {
    const range = VersionRange.fromRequirementString(
        "|| || 1.2.4 || || 1.2.5 ||",
    );
    assertEquals(range.requirements, [
        [{ comparator: "", version: Version.fromSemver("1.2.4") }],
        [{ comparator: "", version: Version.fromSemver("1.2.5") }],
    ]);
});

// satisfied_by() tests
const groups = [
    {
        requirements: "1.2.3",
        tests: [
            { version: "1.2.3", expected: true },
            { version: "1.2.4", expected: false },
            { version: "1.2.2", expected: false },
            { version: "1.2.3-test", expected: true },
        ],
    },
    {
        requirements: "1.2.3 || 1.2.4",
        tests: [
            { version: "1.2.3", expected: true },
            { version: "1.2.4", expected: true },
            { version: "1.2.2", expected: false },
            { version: "1.2.3-test", expected: true },
            { version: "1.2.4+build", expected: true },
        ],
    },
    {
        requirements: "^1.2",
        tests: [
            { version: "1.2", expected: true },
            { version: "1.2.5", expected: true },
            { version: "1.2.6-pre", expected: false },
            { version: "1.3", expected: true },
            { version: "2.0", expected: false },
        ],
    },
    {
        requirements: ">=1.2.3 <=1.3.2",
        tests: [
            { version: "1.2", expected: false },
            { version: "1.2.3", expected: true },
            { version: "1.2.5", expected: true },
            { version: "1.2.6+build", expected: true },
            { version: "1.3.2", expected: true },
            { version: "1.3.3", expected: false },
        ],
    },
    {
        requirements: ">1.2.3 <1.3.2",
        tests: [
            { version: "1.2", expected: false },
            { version: "1.2.3", expected: false },
            { version: "1.2.5", expected: true },
            { version: "1.2.6+build", expected: true },
            { version: "1.3.2", expected: false },
            { version: "1.3.3", expected: false },
        ],
    },
];

groups.forEach((group) => {
    const range = VersionRange.fromRequirementString(
        group.requirements,
    );
    group.tests.forEach((test) => {
        Deno.test(`VersionRange - satisfied_by() - with requirements '${group.requirements}' returns ${test.expected} for '${test.version}'`, () => {
            const version = Version.fromSemver(test.version);
            const result = range.satisfiedBy(version);
            assertEquals(result, test.expected);
        });
    });
});
