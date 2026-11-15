//! Golden fixture for the TypeScript port in `../ts`. Feeds fixed sample sets
//! through the public API and prints, per case, the inputs alongside what this
//! crate emits for them: the `summary_to_json` bytes, the `print_summary`
//! table, `format_ns` strings and `assert_p99_under` messages. The TS parity
//! test replays the same samples and requires identical output.
//!
//! Regenerate after any change to stats or JSON emission:
//!   cargo run --example ts_parity_golden > ../ts/testdata/rust-golden.json

use subms::{
    SubMsBenchAssertion, SubMsPerfHarness, assert_p99_under, format_ns, print_summary, summarize,
    summary_to_json,
};

struct Case {
    name: &'static str,
    sample_cap: Option<usize>,
    stages: Vec<(&'static str, Vec<u64>)>,
}

// Deterministic generator so the fixture regenerates byte-for-byte.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn cases() -> Vec<Case> {
    let mut rng = Lcg(0x5eed);

    // Long-tailed latency shape: a tight body with rare large spikes, so the
    // p99/p999 picks and the log2 histogram all land on distinct values.
    let tail: Vec<u64> = (0..5_000)
        .map(|_| {
            let r = rng.next();
            let body = 800 + r % 400;
            if r % 97 == 0 {
                body * 1_000
            } else if r % 13 == 0 {
                body * 7
            } else {
                body
            }
        })
        .collect();
    let second: Vec<u64> = (0..777).map(|_| 50 + rng.next() % 5_000).collect();

    // Two 32-sample windows with means 16.5 and 15.5: cv = 0.5 / 16 = 0.03125
    // exactly, a tie at the fourth decimal that exposes the rounding rule.
    let mut jitter_tie = Vec::new();
    jitter_tie.extend(std::iter::repeat_n(16u64, 16));
    jitter_tie.extend(std::iter::repeat_n(17u64, 16));
    jitter_tie.extend(std::iter::repeat_n(15u64, 16));
    jitter_tie.extend(std::iter::repeat_n(16u64, 16));

    // One loud window among quiet ones drives cv past 1 so the clamp shows.
    let mut clamped = vec![1u64; 31 * 32];
    clamped.extend(std::iter::repeat_n(1_000_000_000u64, 32));

    // Values near 2^49..2^50: per-window sums and the stage total exceed 2^53,
    // so a port that sums in doubles drifts on mean, stddev and jitter.
    let huge: Vec<u64> = (0..200)
        .map(|_| (1u64 << 49) + (rng.next() << 17) + rng.next() % 1_000)
        .collect();

    // format_ns ties: 1250 -> 1.25 and 1_125_000 -> 1.125 are exact doubles.
    let ties = vec![1_250, 1_250, 1_125_000, 1_050, 999_999, 2_500_000_000_000];

    vec![
        Case {
            name: "empty",
            sample_cap: None,
            stages: vec![("idle", vec![])],
        },
        Case {
            name: "single",
            sample_cap: None,
            stages: vec![("one", vec![42])],
        },
        Case {
            name: "pair",
            sample_cap: None,
            stages: vec![("two", vec![0, 1])],
        },
        Case {
            name: "below_jitter_window",
            sample_cap: None,
            stages: vec![("short", (1..=63).map(|i| i * 3).collect())],
        },
        Case {
            name: "jitter_tie",
            sample_cap: None,
            stages: vec![("tie", jitter_tie)],
        },
        Case {
            name: "jitter_clamped",
            sample_cap: None,
            stages: vec![("spike", clamped)],
        },
        Case {
            name: "zeros",
            sample_cap: None,
            stages: vec![("zero", vec![0; 100])],
        },
        Case {
            name: "long_tail_multi_stage",
            sample_cap: None,
            stages: vec![("z_put", tail), ("a_get", second.clone())],
        },
        Case {
            name: "downsample_cap_100",
            sample_cap: Some(100),
            stages: vec![("ds", second[..700].to_vec())],
        },
        Case {
            name: "downsample_cap_1",
            sample_cap: Some(1),
            stages: vec![("ds1", second[..50].to_vec())],
        },
        Case {
            name: "huge_values",
            sample_cap: None,
            stages: vec![("big", huge)],
        },
        Case {
            name: "format_ties",
            sample_cap: None,
            stages: vec![("fmt", ties)],
        },
    ]
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn join<T: ToString>(xs: &[T]) -> String {
    xs.iter().map(T::to_string).collect::<Vec<_>>().join(",")
}

fn main() {
    let mut out = String::from("{\"cases\":[");
    for (ci, case) in cases().iter().enumerate() {
        let mut h = SubMsPerfHarness::new("ts-parity", "rust");
        if let Some(cap) = case.sample_cap {
            h.set_sample_cap(cap);
        }
        h.input("b_key", "plain");
        h.input("a_key", "quote\" back\\ nl\n tab\t ctl\u{1}");
        h.add_meta("case", case.name);
        for (name, samples) in &case.stages {
            let st = h.stage(name, samples.len());
            for &v in samples {
                st.record(v);
            }
        }
        let summary = summarize(&h);

        let mut json = Vec::new();
        summary_to_json(&summary, &mut json).unwrap();
        let mut table = Vec::new();
        print_summary(&summary, &mut table).unwrap();

        let first = case.stages[0].0;
        let p99 = summary.stage(first).unwrap().p99_ns;
        let below = p99.saturating_sub(1);
        let fail = |bound: u64| {
            assert_p99_under(
                &summary,
                &[SubMsBenchAssertion {
                    stage: first,
                    p99_ns_max: bound,
                }],
            )
        };
        let harness_fail = assert_p99_under(
            &h,
            &[SubMsBenchAssertion {
                stage: first,
                p99_ns_max: below,
            }],
        );
        let missing = assert_p99_under(
            &summary,
            &[SubMsBenchAssertion {
                stage: "nope",
                p99_ns_max: 0,
            }],
        );
        let result = |r: Result<(), String>| match r {
            Ok(()) => "null".to_string(),
            Err(e) => esc(&e),
        };

        if ci > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"name\":{},", esc(case.name)));
        match case.sample_cap {
            Some(c) => out.push_str(&format!("\"sample_cap\":{c},")),
            None => out.push_str("\"sample_cap\":null,"),
        }
        out.push_str("\"stages\":[");
        for (si, (name, samples)) in case.stages.iter().enumerate() {
            if si > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"name\":{},\"samples\":[{}]}}",
                esc(name),
                join(samples)
            ));
        }
        out.push_str("],");
        out.push_str(&format!(
            "\"summary_json\":{},",
            esc(std::str::from_utf8(&json).unwrap())
        ));
        out.push_str(&format!(
            "\"print_summary\":{},",
            esc(std::str::from_utf8(&table).unwrap())
        ));
        out.push_str(&format!(
            "\"assert_at_p99\":{},\"assert_below_p99\":{},\"assert_below_p99_harness\":{},\"assert_missing\":{}",
            result(fail(p99)),
            result(fail(below)),
            result(harness_fail),
            result(missing)
        ));
        out.push('}');
    }

    let fmt_inputs: Vec<u64> = vec![
        0,
        1,
        999,
        1_000,
        1_050,
        1_250,
        1_350,
        12_345,
        999_949,
        999_950,
        999_999,
        1_000_000,
        1_005_000,
        1_125_000,
        1_234_567,
        2_500_000_000_000,
        u64::from(u32::MAX),
        (1u64 << 53) - 1,
    ];
    let fmt_outputs: Vec<String> = fmt_inputs.iter().map(|&n| esc(&format_ns(n))).collect();
    out.push_str(&format!(
        "],\"format_ns\":{{\"inputs\":[{}],\"outputs\":[{}]}}}}",
        join(&fmt_inputs),
        fmt_outputs.join(",")
    ));
    println!("{out}");
}
