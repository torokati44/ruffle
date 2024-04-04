import { injectRuffleAndWait, openTest } from "../../utils.js";
import { expect, use } from "chai";
import chaiHtml from "chai-html";
import fs from "fs";

use(chaiHtml);

describe("Remove object", () => {
    it("loads the test", async () => {
        await openTest(browser, __dirname);
    });

    it("polyfills with ruffle", async () => {
        await injectRuffleAndWait(browser);
        const actual = await browser.$("#test-container").getHTML(false);
        const expected = fs.readFileSync(`${__dirname}/expected.html`, "utf8");
        expect(actual).html.to.equal(expected);
    });

    it("deletes ruffle player by id", async () => {
        await browser.execute(() => {
            const obj = document.getElementById("foo");
            obj.remove();
        });
        const actual = await browser.$("#test-container").getHTML(false);
        const expected = "";
        expect(actual).html.to.equal(expected);
    });
});
