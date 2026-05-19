# subms (Java)

The Java side of the [subms](https://github.com/submillisecond/subms) perf
harness. Zero-dependency JDK-21 library. Records timed samples per stage,
computes percentiles, supports coordinated-omission correction, runs scale
sweeps, and emits a JSON contract byte-equivalent to the Rust side.

See the [repo-root README](../README.md) for the cross-language overview,
JSON-contract spec, and ecosystem links.

## Install

```xml
<dependency>
    <groupId>com.submillisecond</groupId>
    <artifactId>subms</artifactId>
    <version>0.3.0</version>
</dependency>
```

Gradle equivalent:

```kotlin
implementation("com.submillisecond:subms:0.3.0")
```

JDK 21 baseline. No transitive runtime dependencies (JUnit 5 only in test
scope).

## Quickstart

```java
import com.submillisecond.perf.*;
import java.util.List;

SubMsPerfHarness h = new SubMsPerfHarness("my-workload", "java");
h.input("entries", "50000");

SubMsPerfHarness.Stage put = h.stage("put", 50_000);
for (int i = 0; i < 50_000; i++) {
    put.time(() -> { /* hot-path work under test */ });
}

SubMsBenchSummary summary = SubMsBench.summarize(h);
SubMsBench.printSummary(summary, System.out);

SubMsBench.assertP99Under(summary, List.of(
        new SubMsBench.Assertion("put", 1_000_000L)));
```

## Public API surface

| class / method | does |
|---|---|
| `SubMsPerfHarness` | records timed samples per named stage |
| `SubMsPerfHarness.Stage` / `.PacedStage` | per-stage recorder (paced = coordinated-omission corrected) |
| `SubMsBench.summarize(h)` / `summarizeLean(h)` | sorted percentiles + downsampled timeline |
| `SubMsBench.printSummary(s, ps)` | fixed-width percentile table |
| `SubMsBench.summaryToJson(s, ps)` | the stable JSON shape |
| `SubMsBench.runSweep(recipe, params, key)` | multi-run scale curve / feature toggle |
| `SubMsBench.diffSummary(base, cand)` | typed per-stage regression delta |
| `SubMsBench.assertP99Under(target, asserts)` | the cookbook quality gate |
| `SubMsTimer` | autostart stopwatch with named checkpoints |
| `SubMsRecipe` interface | implement for cookbook-style standardised benches |

## Build

```bash
mvn -q test               # run unit tests
mvn -q package            # build the jar
mvn -q install            # publish to local ~/.m2 for downstream test
```

## License

Dual-licensed under [MIT](../LICENSE) or
[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
