import { bridge, type WasmExports } from "./engine";

/** A fake module with the same ABI: echoes the request's `op`, or reports an error. */
function fakeModule(): WasmExports {
  const memory = new WebAssembly.Memory({ initial: 1 });
  let next = 8;
  const alloc = (len: number) => {
    const p = next;
    next += len + 8;
    return p;
  };
  return {
    memory,
    pmx_alloc: alloc,
    pmx_free: () => {},
    pmx_call(ptr, len) {
      const req = JSON.parse(new TextDecoder().decode(new Uint8Array(memory.buffer, ptr, len))) as {
        op: string;
      };
      const reply = new TextEncoder().encode(
        JSON.stringify(req.op === "bad" ? { error: "nope" } : { echoed: req.op }),
      );
      const out = alloc(reply.length);
      new Uint8Array(memory.buffer, out, reply.length).set(reply);
      return (BigInt(out) << 32n) | BigInt(reply.length);
    },
  };
}

describe("engine bridge", () => {
  it("passes JSON both ways", () => {
    expect(bridge(fakeModule())({ op: "range" })).toEqual({ echoed: "range" });
  });

  it("turns engine errors into exceptions", () => {
    expect(() => bridge(fakeModule())({ op: "bad" })).toThrow("metrics engine: nope");
  });
});
