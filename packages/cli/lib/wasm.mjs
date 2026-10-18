// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";

export function instantiate(bytes) {
  assert.ok(bytes.length > 8 && bytes.length <= 10 * 1024 * 1024, "invalid module size");
  const module = new WebAssembly.Module(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), [], "WASM extensions must not import WASI or host functions");
  const instance = new WebAssembly.Instance(module);
  const {memory,manko_alloc,manko_dealloc,manko_dispatch} = instance.exports;
  assert.ok(memory instanceof WebAssembly.Memory);
  assert.ok(memory.buffer.byteLength <= 128 * 1024 * 1024, "initial memory exceeds 128 MiB");
  assert.ok([manko_alloc,manko_dealloc,manko_dispatch].every(value => typeof value === "function"));
  assert.deepEqual([manko_alloc.length,manko_dealloc.length,manko_dispatch.length], [1,2,2], "invalid ABI function arity");
  const dispatch = input => {
    const bytes = Buffer.from(JSON.stringify(input));
    const pointer = manko_alloc(bytes.length) >>> 0;
    const bounds = (pointer,length) => {
      assert.ok(memory.buffer.byteLength <= 128 * 1024 * 1024);
      assert.ok(length <= 24 * 1024 * 1024 && pointer + length <= memory.buffer.byteLength,"malformed memory reference");
    };
    bounds(pointer,bytes.length);
    new Uint8Array(memory.buffer,pointer,bytes.length).set(bytes);
    const packed = manko_dispatch(pointer,bytes.length);
    manko_dealloc(pointer,bytes.length);
    const output = Number(BigInt.asUintN(64,packed) >> 32n), length = Number(packed & 0xffffffffn);
    bounds(output,length);
    const result = JSON.parse(Buffer.from(new Uint8Array(memory.buffer,output,length)).toString());
    manko_dealloc(output,length);
    return result;
  };
  return {instance,dispatch};
}
export function validateModule(bytes,metadata) {
  const {dispatch} = instantiate(bytes);
  assert.deepEqual(dispatch({action:"metadata"}), {id:metadata.id,kind:metadata.kind,apiVersion:"3.0",imageRequestMode:"independent",cachePolicy:"metadata"});
  const connectorKind = metadata.connectorKind ?? metadata.extension?.connectorKind;
  if (connectorKind) {
    const result = dispatch({action:"invoke",invocationID:"connector-validation",method:"connectorInfo",arguments:{}});
    assert.equal(result.ok,true,"connector profile metadata is required");
    assert.deepEqual(result.value,{kind:connectorKind,version:1});
  }
}
