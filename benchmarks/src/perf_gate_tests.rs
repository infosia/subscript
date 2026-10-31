//! Unit tests of the `perf-gate` binary.

use super::*;

#[test]
fn clean_reporting_run_passes_when_thresholds_are_met() {
    let spreads = [NOISE_LIMIT - 0.01];
    assert_eq!(
        decide_run(RunMode::Reporting, true, &spreads),
        RunDecision::Passed
    );
}

#[test]
fn clean_reporting_run_reports_a_missed_threshold() {
    let spreads = [NOISE_LIMIT - 0.01];
    assert_eq!(
        decide_run(RunMode::Reporting, false, &spreads),
        RunDecision::Missed
    );
}

#[test]
fn gate_noise_does_not_void_a_missed_threshold() {
    let spreads = [NOISE_LIMIT + 0.01];
    assert_eq!(
        decide_run(RunMode::Gate, false, &spreads),
        RunDecision::Missed
    );
}

#[test]
fn reporting_noise_voids_before_a_missed_threshold() {
    let spreads = [NOISE_LIMIT + 0.01];
    assert_eq!(
        decide_run(RunMode::Reporting, false, &spreads),
        RunDecision::Void
    );
}

#[test]
fn only_noisy_reporting_timings_are_withheld() {
    let under_limit = NOISE_LIMIT - 0.01;
    let over_limit = NOISE_LIMIT + 0.01;

    assert!(!timing_is_withheld(RunMode::Reporting, under_limit));
    assert!(timing_is_withheld(RunMode::Reporting, over_limit));
    assert!(!timing_is_withheld(RunMode::Gate, under_limit));
    assert!(!timing_is_withheld(RunMode::Gate, over_limit));
}

#[test]
fn an_empty_spread_list_never_voids_either_run() {
    for mode in [RunMode::Reporting, RunMode::Gate] {
        assert_eq!(decide_run(mode, true, &[]), RunDecision::Passed);
        assert_eq!(decide_run(mode, false, &[]), RunDecision::Missed);
    }
}

#[test]
fn excessive_spread_voids_only_the_reporting_run() {
    let spreads = [NOISE_LIMIT + 0.01];
    assert_eq!(
        decide_run(RunMode::Reporting, true, &spreads),
        RunDecision::Void
    );
    assert_eq!(
        decide_run(RunMode::Gate, true, &spreads),
        RunDecision::Passed
    );
}

/// The ship-tier limit of `a22` on aarch64.
const SHIP_LIMIT: f64 = WORKLOADS[0].ship_limit.aarch64;

/// `a22` with the aarch64 ship-tier limit on each host, so that a report
/// test states one figure and one verdict on each host.
static A22: Workload = Workload {
    ship_limit: ShipLimit::uniform(SHIP_LIMIT),
    ..WORKLOADS[0]
};

/// `samples`, given in milliseconds, as durations.
fn durations(samples: &[f64]) -> Vec<Duration> {
    samples
        .iter()
        .map(|sample| Duration::from_secs_f64(sample / 1000.0))
        .collect()
}

/// The statistics of `samples`, given in milliseconds.
fn stats_of(samples: &[f64]) -> Stats {
    Stats::of(&durations(samples)).expect("the samples are not empty")
}

/// The ship-tier comparison of `ship` against `c`, in milliseconds.
fn ship_comparison(mode: RunMode, ship: &[f64], c: &[f64]) -> Comparison {
    compare_ratio(mode, &stats_of(ship), &stats_of(c), SHIP_LIMIT)
        .expect("the baseline figure is not zero")
}

/// Asserts `actual` to the two decimals of a report line.
fn assert_ratio(actual: f64, expected: &str) {
    assert_eq!(format!("{actual:.2}"), expected);
}

const MISS_C: [f64; 11] = [
    5.044, 11.807, 5.018, 6.159, 5.030, 11.245, 7.192, 6.577, 5.050, 12.464, 6.264,
];
const MISS_SHIP: [f64; 11] = [
    8.497, 6.674, 13.860, 11.739, 6.662, 14.336, 13.750, 6.651, 14.099, 6.635, 13.938,
];
const QUIET_C: [f64; 11] = [
    3.872, 3.847, 3.882, 3.858, 3.874, 3.849, 3.973, 3.881, 3.909, 3.849, 3.987,
];
const QUIET_SHIP: [f64; 11] = [
    5.138, 5.230, 5.155, 5.152, 5.141, 5.154, 5.133, 5.129, 5.266, 5.148, 5.169,
];

#[test]
fn a_gate_run_meets_the_loaded_samples_that_a_reporting_run_misses() {
    let gate = ship_comparison(RunMode::Gate, &MISS_SHIP, &MISS_C);
    assert_ratio(gate.figure, "1.32");
    assert!(gate.met);

    let reporting = ship_comparison(RunMode::Reporting, &MISS_SHIP, &MISS_C);
    assert_ratio(reporting.figure, "1.87");
    assert!(!reporting.met);
}

#[test]
fn a_gate_run_meets_quiet_samples() {
    assert!(ship_comparison(RunMode::Gate, &QUIET_SHIP, &QUIET_C).met);
}

#[test]
fn a_gate_run_misses_a_slowdown_of_every_run() {
    let slowed = QUIET_SHIP.map(|sample| sample * 1.32);
    assert!(!ship_comparison(RunMode::Gate, &slowed, &QUIET_C).met);
}

#[test]
fn a_one_shot_figure_follows_the_run_mode() {
    let stats = stats_of(&[
        12.0, 13.0, 12.5, 31.0, 30.0, 32.0, 12.2, 30.5, 31.5, 13.5, 33.0,
    ]);
    let limit = DEV_ITERATION_LIMIT.as_secs_f64();
    assert!(stats.median > limit);
    assert!(stats.min <= limit);

    assert!(compare_time(RunMode::Gate, &stats, limit).met);
    assert!(!compare_time(RunMode::Reporting, &stats, limit).met);
}

#[test]
fn the_zero_time_check_tests_the_figure_that_the_ratio_divides_by() {
    let subject = stats_of(&[1.0, 1.0, 1.0]);
    let baseline = stats_of(&[0.0, 1.0, 1.0]);

    assert!(compare_ratio(RunMode::Gate, &subject, &baseline, SHIP_LIMIT).is_none());
    assert!(compare_ratio(RunMode::Reporting, &subject, &baseline, SHIP_LIMIT).is_some());
}

/// A dev-JIT set with the spread of the §212 miss. Against `MISS_C`, the
/// minimum gives 23.91x and the median gives 27.14x.
const MISS_DEV: [f64; 11] = [
    120.0, 170.0, 125.0, 180.0, 130.0, 190.0, 140.0, 200.0, 150.0, 210.0, 220.0,
];
/// Sets within the noise limit of §9, so that a reporting run prints its
/// figure. Against `QUIET_C`, each minimum meets its limit and each
/// median misses it.
const NARROW_SHIP: [f64; 11] = [5.4, 5.8, 5.9, 6.0, 6.0, 6.0, 6.0, 6.0, 6.1, 6.1, 6.2];
const NARROW_DEV: [f64; 11] = [
    90.0, 96.0, 98.0, 100.0, 100.0, 100.0, 100.0, 100.0, 104.0, 106.0, 110.0,
];
/// A one-shot set whose minimum meets 20 ms and whose median misses it.
const NARROW_ONE_SHOT: [f64; 11] = [
    18.0, 19.0, 20.5, 21.0, 21.0, 21.0, 21.0, 21.5, 22.0, 22.0, 23.0,
];

/// A subject that carries `samples`, given in milliseconds.
fn subject(name: &'static str, samples: &[f64]) -> Subject {
    Subject {
        name,
        span: "the test samples",
        stdout: Vec::new(),
        samples: durations(samples),
        cpu_samples: None,
        warmup: Duration::ZERO,
        warmup_iterations: 0,
        prepare: Vec::new(),
    }
}

/// A one-shot subject that carries `NARROW_ONE_SHOT`.
fn one_shot(name: &'static str) -> OneShotSubject {
    OneShotSubject {
        name,
        span: "the test samples",
        samples: durations(&NARROW_ONE_SHOT),
        warmup: Duration::ZERO,
        warmup_iterations: 0,
    }
}

/// The exit code of a report on the three sample sets, and its threshold
/// lines with each run of spaces collapsed. The first line is the header.
fn threshold_lines(mode: RunMode, c: &[f64], ship: &[f64], dev: &[f64]) -> (ExitCode, Vec<String>) {
    threshold_lines_of(mode, c, ship, subject("dev-JIT", dev))
}

/// `threshold_lines` on `QUIET_C` and `NARROW_SHIP`, for a dev-JIT subject
/// that also carries thread CPU samples.
fn cpu_threshold_lines(mode: RunMode, dev: &[f64], cpu: &[f64]) -> (ExitCode, Vec<String>) {
    let dev = Subject {
        cpu_samples: Some(durations(cpu)),
        ..subject("dev-JIT", dev)
    };
    threshold_lines_of(mode, &QUIET_C, &NARROW_SHIP, dev)
}

/// `threshold_lines` for a built dev-JIT subject.
fn threshold_lines_of(
    mode: RunMode,
    c: &[f64],
    ship: &[f64],
    dev: Subject,
) -> (ExitCode, Vec<String>) {
    let run = WorkloadRun {
        workload: &A22,
        subjects: [subject("C", c), subject("ship-tier", ship), dev],
    };
    let iteration = one_shot("dev-iteration");
    let reload = one_shot("hot-reload");
    let mut report = String::new();
    let code = write_report(
        &mut report,
        &[run],
        &[&iteration, &reload],
        mode,
        DEFAULT_WARMUP,
        DEFAULT_TIMED,
    )
    .expect("the report has each subject");
    let lines = report
        .lines()
        .skip_while(|line| !line.starts_with("thresholds"))
        .take_while(|line| !line.is_empty())
        .filter(|line| !line.contains("scoped by host ISA"))
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    (code, lines)
}

const REPORTING_HEADER: &str = "thresholds (compiler.md §3, pre-registered):";
const GATE_HEADER: &str =
    "thresholds (compiler.md §3, pre-registered), minimum of the timed runs (compiler.md §213):";

#[test]
fn a_gate_report_passes_the_loaded_samples_on_the_minimum() {
    let (code, lines) = threshold_lines(RunMode::Gate, &MISS_C, &MISS_SHIP, &MISS_DEV);
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        lines,
        [
            GATE_HEADER,
            "a22 ship-tier 1.32x of C, limit 1.50x MET",
            "a22 dev-JIT 23.91x of C, limit 25.00x MET",
            "a22 dev-iteration 18.000 ms, limit 20.000 ms MET",
            "a22 hot-reload 18.000 ms, limit 20.000 ms MET",
        ]
    );
}

#[test]
fn a_reporting_report_voids_the_loaded_samples() {
    let (code, lines) = threshold_lines(RunMode::Reporting, &MISS_C, &MISS_SHIP, &MISS_DEV);
    assert_eq!(code, ExitCode::from(2));
    assert_eq!(
        lines,
        [
            REPORTING_HEADER,
            "a22 ship-tier noise, limit 1.50x WITHHELD",
            "a22 dev-JIT noise, limit 25.00x WITHHELD",
            "a22 dev-iteration 21.000 ms, limit 20.000 ms MISSED",
            "a22 hot-reload 21.000 ms, limit 20.000 ms MISSED",
        ]
    );
}

#[test]
fn a_report_compares_the_figure_of_its_run_mode() {
    let (code, lines) = threshold_lines(RunMode::Gate, &QUIET_C, &NARROW_SHIP, &NARROW_DEV);
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        lines,
        [
            GATE_HEADER,
            "a22 ship-tier 1.40x of C, limit 1.50x MET",
            "a22 dev-JIT 23.39x of C, limit 25.00x MET",
            "a22 dev-iteration 18.000 ms, limit 20.000 ms MET",
            "a22 hot-reload 18.000 ms, limit 20.000 ms MET",
        ]
    );

    let (code, lines) = threshold_lines(RunMode::Reporting, &QUIET_C, &NARROW_SHIP, &NARROW_DEV);
    assert_eq!(code, ExitCode::from(1));
    assert_eq!(
        lines,
        [
            REPORTING_HEADER,
            "a22 ship-tier 1.55x of C, limit 1.50x MISSED",
            "a22 dev-JIT 25.81x of C, limit 25.00x MISSED",
            "a22 dev-iteration 21.000 ms, limit 20.000 ms MISSED",
            "a22 hot-reload 21.000 ms, limit 20.000 ms MISSED",
        ]
    );
}

#[test]
fn a_gate_report_misses_a_slowdown_of_every_run() {
    let slowed = QUIET_SHIP.map(|sample| sample * 1.32);
    let (code, lines) = threshold_lines(RunMode::Gate, &QUIET_C, &slowed, &NARROW_DEV);
    assert_eq!(code, ExitCode::from(1));
    assert!(lines.contains(&"a22 ship-tier 1.76x of C, limit 1.50x MISSED".to_string()));
}

#[test]
fn a_gate_report_compares_dev_jit_thread_cpu_time() {
    let slow = NARROW_DEV.map(|sample| sample * 1.5);
    let (code, lines) = cpu_threshold_lines(RunMode::Gate, &slow, &NARROW_DEV);
    assert_eq!(code, ExitCode::SUCCESS);
    let has = |text: &str| lines.iter().any(|line| line.contains(text));
    assert!(has("thread CPU time over C wall time:"));
    assert!(has("a22 dev-JIT 23.39x of C, limit 25.00x MET"));
    assert!(has("wall minimum 35.09x of C, CPU minimum 90.000 ms"));

    let (code, lines) = cpu_threshold_lines(RunMode::Gate, &NARROW_DEV, &slow);
    assert_eq!(code, ExitCode::from(1));
    let missed = "a22 dev-JIT 35.09x of C, limit 25.00x MISSED";
    assert!(lines.iter().any(|line| line == missed));
}

#[test]
fn a_reporting_report_compares_dev_jit_wall_time() {
    let (code, lines) = cpu_threshold_lines(RunMode::Reporting, &NARROW_DEV, &[1.0]);
    assert_eq!(code, ExitCode::from(1));
    assert_eq!(lines.first().map(String::as_str), Some(REPORTING_HEADER));
    let missed = "a22 dev-JIT 25.81x of C, limit 25.00x MISSED";
    assert!(lines.iter().any(|line| line == missed));
}
