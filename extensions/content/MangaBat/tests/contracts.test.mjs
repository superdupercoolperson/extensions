import test from "node:test";
import {resolve} from "node:path";
import {assertContentExtension} from "../../../../packages/cli/lib/contracts.mjs";
test("MangaBat declares a valid API 2.0 extension",()=>assertContentExtension(resolve(import.meta.dirname,".."),"MangaBat"));
