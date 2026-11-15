// Typed counterpart of the subms JSON contract. Property names are camelCase;
// summaryToJson writes the snake_case keys the contract defines.

/** One `stages.<name>` entry. */
export interface SubMsStageSummary {
  readonly name: string;
  readonly count: number;
  readonly p50Ns: number;
  readonly p99Ns: number;
  readonly p999Ns: number;
  readonly maxNs: number;
  readonly meanNs: number;
  /** Sample standard deviation (n - 1), 0 when count < 2. */
  readonly stddevNs: number;
  /** 64 log2 buckets; entry i counts samples in [2^i, 2^(i+1)) ns. */
  readonly cdfBucketsNs: readonly number[];
  /** CV of 32-sample window means, clamped to [0, 1]. High values mean the measuring rig was noisy, separate from the algorithm's own tail. */
  readonly jitterScore: number;
  /** Chronological timeline, downsampled to the harness sample cap. */
  readonly samplesNs: readonly number[];
}

/** One bench run. Stage order is registration order. */
export interface SubMsBenchSummary {
  workload: string;
  lang: string;
  timestamp: string;
  /** Last core the process ran on (Linux /proc/self/stat field 39), null elsewhere. */
  cpuCore: number | null;
  /** Allowed-CPU list (Linux Cpus_allowed_list), e.g. "1" when pinned. Null elsewhere. */
  cpuAffinity: string | null;
  inputs: ReadonlyMap<string, string>;
  meta: ReadonlyMap<string, string>;
  stages: readonly SubMsStageSummary[];
}

/** A stage-level p99 bound for assertP99Under. */
export interface SubMsBenchAssertion {
  readonly stage: string;
  readonly p99NsMax: number;
}
