use std::time::Duration;

use super::{DbLatencyOutcome, EvalOutcome};

pub(super) fn print_quality_report(filler_count: usize, outcomes: &[EvalOutcome]) {
    let top1 = outcomes
        .iter()
        .filter(|outcome| outcome.rank == Some(1))
        .count();
    let top3 = outcomes
        .iter()
        .filter(|outcome| outcome.rank.is_some_and(|rank| rank <= 3))
        .count();
    let mode_checked = outcomes
        .iter()
        .filter(|outcome| outcome.expected_mode.is_some())
        .count();
    let mode_ok = outcomes
        .iter()
        .filter(|outcome| {
            outcome.expected_mode.is_some() && outcome.mode_used.as_deref() == outcome.expected_mode
        })
        .count();
    let mrr = outcomes
        .iter()
        .map(|outcome| outcome.rank.map_or(0.0, |rank| 1.0 / rank as f64))
        .sum::<f64>()
        / outcomes.len() as f64;

    println!("\nclipmem search benchmark");
    println!("archive_filler_snapshots={filler_count}");
    println!(
        "quality total={} top1={}/{} top3={}/{} mrr={:.3} mode={}/{}",
        outcomes.len(),
        top1,
        outcomes.len(),
        top3,
        outcomes.len(),
        mrr,
        mode_ok,
        mode_checked
    );
    println!(
        "{:<38} {:<7} {:>5} {:>11} {:>11} {:>8} {:>8} {:>10}",
        "case", "cmd", "rank", "expected", "top1", "mode", "med_ms", "p95_ms"
    );
    for outcome in outcomes {
        let rank = outcome
            .rank
            .map(|value| value.to_string())
            .unwrap_or_else(|| "MISS".to_string());
        println!(
            "{:<38} {:<7} {:>5} {:>11} {:>11} {:>8} {:>8.3} {:>10.3}",
            outcome.name,
            outcome.command.as_str(),
            rank,
            outcome.expected_id,
            outcome
                .top1_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            outcome.mode_used.as_deref().unwrap_or("-"),
            ms(outcome.median_cli_latency),
            ms(outcome.p95_cli_latency)
        );
    }
}

pub(super) fn print_db_latency_report(outcomes: &[DbLatencyOutcome]) {
    println!("\ndatabase search latency");
    println!(
        "{:<24} {:<7} {:>8} {:>10} {:>8}",
        "case", "mode", "med_ms", "p95_ms", "results"
    );
    for outcome in outcomes {
        println!(
            "{:<24} {:<7} {:>8.3} {:>10.3} {:>8}",
            outcome.name,
            outcome.mode.as_str(),
            ms(outcome.median_latency),
            ms(outcome.p95_latency),
            outcome.result_count
        );
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
