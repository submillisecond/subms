use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, benchmark};

struct NoopRecipe;

impl SubMsRecipe for NoopRecipe {
    fn name(&self) -> &str {
        "noop"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let stage = h.stage("noop", params.entries);
        for _ in 0..params.entries {
            stage.time(|| {});
        }
    }
}

#[test]
fn benchmark_produces_expected_json_shape() {
    let params = SubMsBenchParams {
        entries: 50,
        warmup: 0,
        seed: 0,
    };
    let h = benchmark(&NoopRecipe, &params);

    let mut buf = Vec::new();
    h.write_json(&mut buf).unwrap();
    let json = String::from_utf8(buf).unwrap();

    assert!(json.contains("\"workload\":\"noop\""));
    assert!(json.contains("\"lang\":\"rust\""));
    assert!(json.contains("\"entries\":\"50\""));
    assert!(json.contains("\"warmup\":\"0\""));
    assert!(json.contains("\"seed\":\"0\""));
    assert!(json.contains("\"noop\":{"));
    assert!(json.contains("\"count\":50"));
}

#[test]
fn bench_params_default_is_sensible() {
    let d = SubMsBenchParams::default();
    assert!(d.entries > 0);
    assert!(d.warmup <= d.entries);
}

#[test]
fn lcg_reexport_works() {
    let mut rng = SubMsLcg::new(123);
    let _ = rng.bounded(10);
}
