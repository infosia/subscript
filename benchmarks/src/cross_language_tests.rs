use super::*;

/// Builds a measurement without executing a workload.
fn measurement(samples_s: &[f64]) -> Measured {
    Measured {
        checksum: 1,
        median_s: stats(samples_s).map_or(0.0, |(median, _, _)| median),
        samples_s: samples_s.to_vec(),
        warmup_s: 0.2,
        warmup_iterations: 3,
    }
}

#[test]
fn quartiles_interpolate_eleven_samples() {
    let x = [
        90.0, 92.0, 94.0, 96.0, 98.0, 100.0, 102.0, 104.0, 106.0, 108.0, 110.0,
    ];
    let mut reversed = x;
    reversed.reverse();
    let m = measurement(&reversed);
    assert_eq!(m.median_s, 100.0);
    assert_eq!(
        m.quartiles(),
        Some((x[2] + 0.5 * (x[3] - x[2]), x[7] + 0.5 * (x[8] - x[7])))
    );
    assert_eq!(m.iqr_fraction(), Some(0.10));
    assert_eq!(measurement(&[5.0]).quartiles(), Some((5.0, 5.0)));
}

#[test]
fn isolated_outlier_does_not_withhold_stable_median() {
    let m = measurement(&[
        98.0, 99.0, 99.0, 100.0, 100.0, 100.0, 101.0, 101.0, 102.0, 102.0, 155.0,
    ]);
    assert_eq!(m.median_s, 100.0);
    assert_eq!(m.iqr_fraction(), Some(0.02));
    assert!(!m.noisy());
}

#[test]
fn noise_gate_compares_iqr_to_limit() {
    for (half_width, noisy) in [(8.0, true), (7.0, false), (7.5, false)] {
        let mut samples = vec![100.0 - half_width; 5];
        samples.push(100.0);
        samples.extend([100.0 + half_width; 5]);
        let m = measurement(&samples);
        assert_eq!(m.iqr_fraction(), Some(2.0 * half_width / 100.0));
        assert_eq!(m.noisy(), noisy);
    }
}

#[test]
fn invalid_samples_are_noisy_but_nonpositive_medians_are_not() {
    for samples in [
        vec![],
        vec![f64::NAN],
        vec![1.0, f64::INFINITY],
        vec![f64::NEG_INFINITY],
        vec![-1.0],
    ] {
        let m = measurement(&samples);
        assert_eq!(m.quartiles(), None);
        assert_eq!(m.iqr_fraction(), None);
        assert!(m.noisy());
    }
    for median in [0.0, -1.0] {
        let mut m = measurement(&[0.0; 11]);
        m.median_s = median;
        assert_eq!(m.iqr_fraction(), Some(0.0));
        assert!(!m.noisy());
    }
}

#[test]
fn reports_use_iqr_and_preserve_ranges() {
    let machine = Machine {
        arch: "test".into(),
        os: "test".into(),
        cpu: "test".into(),
        cores: "1".into(),
        power: "test".into(),
    };
    let mut rows = [WorkloadResult {
        id: "sort".into(),
        checksum: Some(1),
        matched: true,
        outcomes: vec![(
            "C".into(),
            Outcome::Ok(measurement(&[
                90.0, 92.0, 92.0, 92.0, 99.0, 100.0, 101.0, 108.0, 108.0, 108.0, 110.0,
            ])),
        )],
    }];
    let readme = render_readme(&rows, &machine, &[], "test", 3, 11);
    assert!(readme.contains("Noise: interquartile range wider than 15% of the median"));
    assert!(readme.contains("sort/C (IQR 16.0% of median)"));
    assert!(readme.contains("invalid (noise)"));
    let json = render_json(&rows, &machine, &[], "test", 3, 11);
    assert!(json.contains("\"ranges_s\": {\n        \"C\": [90.000000000, 110.000000000],"));
    assert!(json.contains("\"quartiles_s\": {\n        \"C\": [92.000000000, 108.000000000],\n        \"subscript-ship\": null,"));
    rows[0].outcomes = vec![("C".into(), Outcome::Ok(measurement(&[100.0; 11])))];
    let readme = render_readme(&rows, &machine, &[], "test", 3, 11);
    assert!(readme.contains(
        "Noise: no recorded sample set has an interquartile range wider than 15% of the median."
    ));
}

#[test]
fn writes_record_when_all_workloads_are_measured() {
    assert!(writes_the_record(10, 10));
}

#[test]
fn does_not_write_record_when_fewer_workloads_are_measured() {
    assert!(!writes_the_record(1, 10));
}

#[test]
fn does_not_write_record_when_no_workloads_are_measured() {
    assert!(!writes_the_record(0, 0));
}

#[test]
fn does_not_write_record_when_one_subject_has_an_error() {
    let outcomes = vec![
        (
            "C".to_string(),
            Outcome::Unavailable("not installed".to_string()),
        ),
        (
            "subscript-ship".to_string(),
            Outcome::Error("compile failed".to_string()),
        ),
    ];
    let measured = usize::from(workload_has_all_present_timings(&outcomes));
    assert!(!writes_the_record(measured, 1));
}
