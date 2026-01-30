import { assertEquals, assert } from "jsr:@std/assert@1";
import { Version } from "../src/version.ts";

// Each row should be a list of compatible versions.
// Earlier entries in a row should be "greater than" later entries in the same row.
// Earlier rows should be "greater than" later rows.
// Rows should be incompatible with other rows.
const testMatrix = [
    ["3.6-pre1", "3.5.4", "3.2.3", "3"],
    ["2.5", "2.2.6", "2.2.4", "2.2.3"],
    [
        "1",
        "1-rc.1",
        "1-beta.11",
        "1-beta.2",
        "1-beta.01",
        "1-beta",
        "1-alpha.beta",
        "1-alpha.1",
        "1-alpha",
    ],
    ["0.1.2", "0.1.1-dev", "0.1"],
    ["0.0.2+build", "0.0.2+124", "0.0.2+123", "0.0.2", "0.0.2-dev"],
    ["0.0.1", "0.0.1-dev", "0.0.1-5", "0.0.1-2"],
];

function flatten<T>(arr: T[][]): T[] {
    return arr.reduce((accumulator, value) => accumulator.concat(value), []);
}

Deno.test("Version - from_semver() - should parse valid semver strings", () => {
    assertEquals(
        Version.fromSemver("1.2"),
        new Version(1, 2, 0, null, null),
    );
    assertEquals(
        Version.fromSemver("1.2.3"),
        new Version(1, 2, 3, null, null),
    );
    assertEquals(
        Version.fromSemver("1.09.3"),
        new Version(1, 9, 3, null, null),
    );
    assertEquals(
        Version.fromSemver("1.2.3-pr"),
        new Version(1, 2, 3, ["pr"], null),
    );
    assertEquals(
        Version.fromSemver("1.2.3-pr1.pr2"),
        new Version(1, 2, 3, ["pr1", "pr2"], null),
    );
    assertEquals(
        Version.fromSemver("1.2.3+build"),
        new Version(1, 2, 3, null, ["build"]),
    );
    assertEquals(
        Version.fromSemver("1.2.3+build1.build2"),
        new Version(1, 2, 3, null, ["build1", "build2"]),
    );
    assertEquals(
        Version.fromSemver("1-pr1.pr2+build1.build2"),
        new Version(1, 0, 0, ["pr1", "pr2"], ["build1", "build2"]),
    );
});

Deno.test("Version - is_compatible_with() - is compatible with similar versions", () => {
    for (const test of testMatrix) {
        for (const a of test) {
            for (const b of test) {
                assert(
                    Version.fromSemver(a).isCompatibleWith(
                        Version.fromSemver(b),
                    ),
                    `${a} is compatible with ${b}`,
                );
            }
        }
    }
});

Deno.test("Version - is_compatible_with() - is not compatible with other versions", () => {
    for (const test of testMatrix) {
        for (const a of test) {
            for (const otherTest of testMatrix) {
                if (test === otherTest) {
                    continue;
                }
                for (const b of otherTest) {
                    assert(
                        !Version.fromSemver(a).isCompatibleWith(
                            Version.fromSemver(b),
                        ),
                        `${a} is not compatible with ${b}`,
                    );
                }
            }
        }
    }
});

Deno.test("Version - has_precedence_over() - returns true when it should", () => {
    const tests = flatten(testMatrix);
    for (let a = 0; a < tests.length; a++) {
        for (let b = a + 1; b < tests.length; b++) {
            assert(
                Version.fromSemver(tests[a]!).hasPrecedenceOver(
                    Version.fromSemver(tests[b]!),
                ),
                `${tests[a]} has precedence over ${tests[b]}`,
            );
        }
    }
});

Deno.test("Version - has_precedence_over() - returns false when it should", () => {
    const tests = flatten(testMatrix).reverse();
    for (let a = 0; a < tests.length; a++) {
        for (let b = a + 1; b < tests.length; b++) {
            assert(
                !Version.fromSemver(tests[a]!).hasPrecedenceOver(
                    Version.fromSemver(tests[b]!),
                ),
                `${tests[a]} doesn't have precedence over ${tests[b]}`,
            );
        }
    }
});

Deno.test("Version - is_equal() - returns true when it should", () => {
    const tests = flatten(testMatrix);
    for (const version of tests) {
        assert(
            Version.fromSemver(version).isEqual(
                Version.fromSemver(version),
            ),
            `${version} is equal to itself`,
        );
    }
});

Deno.test("Version - is_equal() - returns false when it should", () => {
    const tests = flatten(testMatrix).reverse();
    for (let a = 0; a < tests.length; a++) {
        for (let b = a + 1; b < tests.length; b++) {
            if (
                tests[a]!.indexOf("+") > -1 ||
                tests[b]!.indexOf("+") > -1 ||
                tests[a]!.indexOf("-") > -1 ||
                tests[b]!.indexOf("-") > -1
            ) {
                // Skip "builds" and "identifiers" for purposes of this test.
                continue;
            }
            assert(
                !Version.fromSemver(tests[a]!).isEqual(
                    Version.fromSemver(tests[b]!),
                ),
                `${tests[a]} does not equal ${tests[b]}`,
            );
        }
    }
});

Deno.test("Version - is_stable_or_compatible_prerelease() - returns true for own versions", () => {
    const tests = flatten(testMatrix);
    for (const version of tests) {
        assert(
            Version.fromSemver(version).isStableOrCompatiblePrerelease(
                Version.fromSemver(version),
            ),
            `${version} is compatible with itself`,
        );
    }
});

Deno.test("Version - is_stable_or_compatible_prerelease() - returns true for compatible pre-releases", () => {
    const tests = ["1.2.3", "1.2.3-alpha", "1.2.3-beta1.build2"];
    for (const a of tests) {
        for (const b of tests) {
            assert(
                Version.fromSemver(a).isStableOrCompatiblePrerelease(
                    Version.fromSemver(b),
                ),
                `${a} is compatible with ${b}`,
            );
        }
    }
});

Deno.test("Version - is_stable_or_compatible_prerelease() - returns false for incompatible pre-releases", () => {
    const tests = ["1-dev", "1.2-alpha", "1.2.3-beta1.build2"];
    for (const a of tests) {
        for (const b of tests) {
            if (a === b) {
                continue;
            }
            assert(
                !Version.fromSemver(a).isStableOrCompatiblePrerelease(
                    Version.fromSemver(b),
                ),
                `${a} is not compatible with ${b}`,
            );
        }
    }
});
