import { readFileSync } from "node:fs";
import { stdout } from "node:process";

import { formatNs, toFixedHalfEven } from "./format.ts";
import { type SubMsPerfHarness } from "./harness.ts";
import { cdfBuckets, jitterScore, mean, percentile, stddev } from "./stats.ts";
import {
  type SubMsBenchAssertion,
  type SubMsBenchSummary,
  type SubMsStageSummary,
} from "./summary.ts";

/** Summarise every stage in registration order, with the downsampled timeline. No warmup samples are dropped. */
export function summarize(h: SubMsPerfHarness): SubMsBenchSummary {
  const [cpuCore, cpuAffinity] = cpuPlacement();
  return {
    workload: h.workload,
    lang: h.lang,
    timestamp: h.timestamp(),
    cpuCore,
    cpuAffinity,
    inputs: new Map(h.inputs),
    meta: new Map(h.meta),
    stages: h.stages.map((s) => summarizeStage(s.name, s.samples, h.sampleCap)),
  };
}

function summarizeStage(
  name: string,
  chronological: readonly number[],
  sampleCap: number,
): SubMsStageSummary {
  // Typed-array sort is numeric; Array#sort would compare as strings.
  const sorted = Float64Array.from(chronological).sort();
  return {
    name,
    count: sorted.length,
    p50Ns: percentile(sorted, 0.5),
    p99Ns: percentile(sorted, 0.99),
    p999Ns: percentile(sorted, 0.999),
    maxNs: sorted.length === 0 ? 0 : sorted[sorted.length - 1],
    meanNs: mean(chronological),
    stddevNs: stddev(chronological),
    cdfBucketsNs: cdfBuckets(chronological),
    jitterScore: jitterScore(chronological),
    samplesNs: downsample(chronological, sampleCap),
  };
}

/** Every step-th point from the start, step = max(1, floor(n / cap)). Can return a little more than `cap` points. */
export function downsample(chronological: readonly number[], cap: number): number[] {
  const n = chronological.length;
  const step = Math.max(1, Math.floor(n / Math.max(1, cap)));
  const out: number[] = [];
  for (let i = 0; i < n; i += step) out.push(chronological[i]);
  return out;
}

// Best effort: a bench never fails over provenance, so any read or parse
// problem, or a non-Linux host, yields nulls.
function cpuPlacement(): [number | null, string | null] {
  let core: number | null = null;
  let affinity: string | null = null;
  try {
    const stat = readFileSync("/proc/self/stat", "utf8");
    // comm (field 2) may contain spaces and parens; fields after its closing
    // paren start at field 3, so `processor` (field 39) is index 36.
    const field = stat.slice(stat.lastIndexOf(")") + 1).trim().split(/\s+/)[36];
    if (field !== undefined && /^\d+$/.test(field)) {
      const n = Number(field);
      if (n <= 0xffffffff) core = n;
    }
  } catch {
    // not Linux
  }
  try {
    const status = readFileSync("/proc/self/status", "utf8");
    for (const line of status.split("\n")) {
      if (line.startsWith("Cpus_allowed_list:")) {
        affinity = line.slice("Cpus_allowed_list:".length).trim();
        break;
      }
    }
  } catch {
    // not Linux
  }
  return [core, affinity];
}

/** Writable sink for printSummary. process.stdout satisfies it. */
export interface SubMsTextSink {
  write(chunk: string): unknown;
}

/** Fixed-width percentile table, one row per stage. Same bytes as Rust's print_summary. */
export function printSummary(s: SubMsBenchSummary, out: SubMsTextSink = stdout): void {
  const row = (cols: string[]): string =>
    `  ${cols[0]}${pad(cols[0])}${cols
      .slice(1)
      .map((c) => `  ${pad(c)}${c}`)
      .join("")}\n`;
  let text = row(["stage", "p50", "p99", "p99.9", "max", "mean"]);
  for (const st of s.stages) {
    text += row([
      st.name,
      formatNs(st.p50Ns),
      formatNs(st.p99Ns),
      formatNs(st.p999Ns),
      formatNs(st.maxNs),
      formatNs(st.meanNs),
    ]);
  }
  out.write(text);
}

// Rust's {:<9} and {:>9} count chars (code points); padEnd counts UTF-16 units.
function pad(s: string): string {
  return " ".repeat(Math.max(0, 9 - [...s].length));
}

/**
 * Throws on the first stage whose p99 exceeds its bound, or that does not
 * exist, with the message Rust returns as its Err. Accepts a summary or, as
 * Rust does, the raw harness (p99 is then computed from all its samples).
 */
export function assertP99Under(
  target: SubMsBenchSummary | SubMsPerfHarness,
  assertions: readonly SubMsBenchAssertion[],
): void {
  for (const a of assertions) {
    const p99 = lookupP99(target, a.stage);
    if (p99 === undefined) throw new Error(`stage '${a.stage}' not found`);
    if (p99 > a.p99NsMax) {
      throw new Error(`stage '${a.stage}' p99 = ${p99} ns exceeded limit ${a.p99NsMax} ns`);
    }
  }
}

function lookupP99(target: SubMsBenchSummary | SubMsPerfHarness, stage: string): number | undefined {
  if ("stageByName" in target) {
    const st = target.stageByName(stage);
    return st === undefined ? undefined : percentile(Float64Array.from(st.samples).sort(), 0.99);
  }
  return target.stages.find((s) => s.name === stage)?.p99Ns;
}

/**
 * The subms JSON contract, byte-equivalent to Rust's summary_to_json for the
 * same summary, trailing newline included, so the string can be written
 * straight to perf/<lang>.json.
 */
export function summaryToJson(s: SubMsBenchSummary): string {
  let out = "{";
  out += `${jsonStr("workload")}:${jsonStr(s.workload)},`;
  out += `${jsonStr("lang")}:${jsonStr(s.lang)},`;
  out += `${jsonStr("timestamp")}:${jsonStr(s.timestamp)},`;
  out += `"inputs":${jsonMap(s.inputs)},`;
  out += `"meta":${jsonMap(s.meta)},`;
  out += `"cpu":${cpuJson(s.cpuCore, s.cpuAffinity)},`;
  out += `"stages":{`;
  out += s.stages.map((st) => `${jsonStr(st.name)}:${stageJson(st)}`).join(",");
  out += "}}\n";
  return out;
}

function cpuJson(core: number | null, affinity: string | null): string {
  if (core === null && affinity === null) return "null";
  return `{"core":${core === null ? "null" : core},"affinity":${affinity === null ? "null" : jsonStr(affinity)}}`;
}

function stageJson(st: SubMsStageSummary): string {
  return (
    `{"count":${st.count},` +
    `"p50_ns":${st.p50Ns},` +
    `"p99_ns":${st.p99Ns},` +
    `"p999_ns":${st.p999Ns},` +
    `"max_ns":${st.maxNs},` +
    `"mean_ns":${st.meanNs},` +
    `"stddev_ns":${st.stddevNs},` +
    `"jitter_score":${toFixedHalfEven(st.jitterScore, 4)},` +
    `"cdf_buckets_ns":[${st.cdfBucketsNs.join(",")}],` +
    `"samples_ns":[${st.samplesNs.join(",")}]}`
  );
}

// Rust's escaper: only quote, backslash and C0 controls; everything else,
// including U+2028 and DEL, passes through raw.
function jsonStr(s: string): string {
  let out = '"';
  for (const ch of s) {
    const c = ch.codePointAt(0) ?? 0;
    if (ch === '"') out += '\\"';
    else if (ch === "\\") out += "\\\\";
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else if (c < 0x20) out += `\\u${c.toString(16).padStart(4, "0")}`;
    else out += ch;
  }
  return `${out}"`;
}

// BTreeMap iterates keys in UTF-8 byte order, which is code point order.
// Default string comparison is UTF-16 code unit order and disagrees once
// astral characters meet U+E000..U+FFFF.
function jsonMap(m: ReadonlyMap<string, string>): string {
  const keys = [...m.keys()].sort(compareCodePoints);
  return `{${keys.map((k) => `${jsonStr(k)}:${jsonStr(m.get(k) ?? "")}`).join(",")}}`;
}

function compareCodePoints(a: string, b: string): number {
  const ia = a[Symbol.iterator]();
  const ib = b[Symbol.iterator]();
  for (;;) {
    const x = ia.next();
    const y = ib.next();
    if (x.done || y.done) return x.done ? (y.done ? 0 : -1) : 1;
    const cx = x.value.codePointAt(0) ?? 0;
    const cy = y.value.codePointAt(0) ?? 0;
    if (cx !== cy) return cx - cy;
  }
}
