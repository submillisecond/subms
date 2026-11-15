import { hrtime } from "node:process";

/** Stamped into every run's meta. A capture that does not name the harness release that measured it cannot be reproduced. */
export const HARNESS_VERSION = "0.1.0";

const DEFAULT_SAMPLE_CAP = 500;

/** Per-stage sample buffer. Samples are integer nanoseconds. */
export class SubMsStage {
  readonly #name: string;
  readonly #samples: number[] = [];

  /** @internal Created through {@link SubMsPerfHarness.stage}. */
  constructor(name: string) {
    this.#name = name;
  }

  get name(): string {
    return this.#name;
  }

  /** Chronological samples recorded so far. */
  get samples(): readonly number[] {
    return this.#samples;
  }

  /** Record a duration measured elsewhere. Must be a non-negative safe integer, the domain of Rust's u64 that a double holds exactly. */
  record(ns: number): void {
    if (!Number.isSafeInteger(ns) || ns < 0) {
      throw new RangeError(`sample must be a non-negative safe integer ns, got ${ns}`);
    }
    this.#samples.push(ns);
  }

  /** Time a synchronous call and record it. A throw propagates and records nothing. */
  time<R>(fn: () => R): R {
    const t0 = hrtime.bigint();
    const r = fn();
    this.#samples.push(Number(hrtime.bigint() - t0));
    return r;
  }

  /** Time an async call from invocation to settlement, so time spent queued on the event loop counts. */
  async timeAsync<R>(fn: () => Promise<R>): Promise<R> {
    const t0 = hrtime.bigint();
    const r = await fn();
    this.#samples.push(Number(hrtime.bigint() - t0));
    return r;
  }

  /**
   * Run `op` `warmup` times untimed, then record `measured` timed calls. `op`
   * gets the iteration index on both passes. V8 needs this the way HotSpot
   * does: a function starts interpreted and is only optimised once hot, so
   * without a warmup the first samples measure the tier-up.
   */
  warmThenTime(warmup: number, measured: number, op: (i: number) => void): void {
    for (let i = 0; i < warmup; i++) op(i);
    for (let i = 0; i < measured; i++) {
      const t0 = hrtime.bigint();
      op(i);
      this.#samples.push(Number(hrtime.bigint() - t0));
    }
  }
}

/** A workload run: identity, inputs, meta and raw per-stage samples. Call `summarize` to analyse it. */
export class SubMsPerfHarness {
  readonly #workload: string;
  readonly #lang: string;
  readonly #inputs = new Map<string, string>();
  readonly #meta = new Map<string, string>([["harness_version", HARNESS_VERSION]]);
  readonly #stages: SubMsStage[] = [];
  #sampleCap = DEFAULT_SAMPLE_CAP;

  constructor(workload: string, lang: string) {
    this.#workload = workload;
    this.#lang = lang;
  }

  get workload(): string {
    return this.#workload;
  }

  get lang(): string {
    return this.#lang;
  }

  get inputs(): ReadonlyMap<string, string> {
    return this.#inputs;
  }

  get meta(): ReadonlyMap<string, string> {
    return this.#meta;
  }

  get stages(): readonly SubMsStage[] {
    return this.#stages;
  }

  /** Max points kept in each stage's emitted samples_ns. Default 500, floor 1. */
  get sampleCap(): number {
    return this.#sampleCap;
  }

  setSampleCap(cap: number): this {
    this.#sampleCap = Math.max(1, Math.floor(cap));
    return this;
  }

  input(key: string, value: string): this {
    this.#inputs.set(key, value);
    return this;
  }

  addMeta(key: string, value: string): this {
    this.#meta.set(key, value);
    return this;
  }

  /**
   * Register a new stage. `expectedCount` is the Rust capacity hint and is
   * unused here. Names are not deduplicated, matching Rust: two stages with
   * one name emit a duplicate JSON key.
   */
  stage(name: string, _expectedCount?: number): SubMsStage {
    const s = new SubMsStage(name);
    this.#stages.push(s);
    return s;
  }

  stageByName(name: string): SubMsStage | undefined {
    return this.#stages.find((s) => s.name === name);
  }

  /** ISO-8601 UTC, seconds precision, as written to the JSON timestamp. */
  timestamp(): string {
    return `${new Date().toISOString().slice(0, 19)}Z`;
  }
}
