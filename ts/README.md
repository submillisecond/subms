# subms (TypeScript)

The TypeScript side of the [subms](https://github.com/submillisecond/subms) perf
harness, for Node 22.6 and later. It times stages in a Node process and writes
the same JSON the Rust and Java harnesses write, with the same statistics for
the same samples, so a TypeScript run renders on submillisecond.com like any
other. No runtime dependencies.

## Run it

```sh
cd ts
npm test
```

Node 22.6 to 22.17 needs the `--experimental-strip-types` flag the test script
passes; later releases strip types by default. There is no build step yet, so
the package exports `src/index.ts` and expects a runtime that loads TypeScript.

## Use it

```ts
import { writeFileSync } from "node:fs";
import { SubMsPerfHarness, assertP99Under, printSummary, summarize, summaryToJson } from "./src/index.ts";

const h = new SubMsPerfHarness("lsm-tree", "typescript");
h.input("entries", "50000");
h.addMeta("store", "memtable");

const put = h.stage("put", 50_000);
put.warmThenTime(10_000, 50_000, (i) => tree.put(keys[i % keys.length]));

const get = h.stage("get");
for (const k of keys) get.time(() => tree.get(k));

const summary = summarize(h);
printSummary(summary);
writeFileSync("perf/typescript.json", summaryToJson(summary));
assertP99Under(summary, [{ stage: "put", p99NsMax: 1_000_000 }]);
```

`stage.record(ns)` takes a duration you measured yourself. `timeAsync` times a
promise to settlement, which includes any time it spent queued on the event
loop. Timing uses `process.hrtime.bigint()`.

## The JSON contract

It is the Rust one, documented at the top of `../rust/src/lib.rs`, and
`summaryToJson` returns the bytes Rust's `summary_to_json` writes, trailing
newline included. The statistics follow `../rust/src/stats.rs` exactly:
nearest-rank percentiles by truncation, a truncated integer mean, a rounded n-1
standard deviation around that mean, 64 log2 buckets, and the jitter score over
32-sample windows. `jitter_score` and the printed table round exact ties to
even, as Rust's formatter does, where `toFixed` would round them up.

`meta.harness_version` carries this package's version. The site's language list
does not include TypeScript yet, so pick the `lang` string with whoever renders
the page.

Parity is checked against the crate itself. `testdata/rust-golden.json` holds
fixed sample sets and what Rust emits for them; `src/bench.test.ts` replays
every case and requires identical JSON, table, `format_ns` strings and assertion
messages. Regenerate it after any change to the Rust stats or emitter:

```sh
cd rust
cargo run --example ts_parity_golden > ../ts/testdata/rust-golden.json
```

## What is left out

This is the core a single-process bench needs. Not ported: `summarize_lean`,
`summarize_skipping` and `summarize_windowed`; paced (coordinated-omission
corrected) stages; sweeps and diffs; growth and feature manifests; observers and
stage kinds; `SubMsTimer`; recipes, bench params, config and env parsing; and
CPU pinning. The `cpu` block is filled from `/proc` on Linux and is `null`
elsewhere, as in Rust. Samples must be non-negative integers up to 2^53 - 1 ns
(about 104 days), the part of Rust's u64 range a JavaScript number holds exactly;
`record` throws outside it.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
