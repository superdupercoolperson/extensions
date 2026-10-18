/* SPDX-License-Identifier: Apache-2.0 */
import test from "node:test";
import {resolve} from "node:path";
import {assertContentExtension} from "../../../../packages/cli/lib/contracts.mjs";
test("MangaFire package declares a valid content extension", () => assertContentExtension(resolve(import.meta.dirname, ".."), "MangaFire"));
