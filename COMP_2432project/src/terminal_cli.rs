use std::io::{self, Write};

use crate::api::{AppState, SystemState, TaskPriority};
use crate::coordinator::builder::{effective_demo_task_count, effective_worker_count};
use crate::types::config::{Config, SchedulerKind};

const SEP: &str = "========================================================================";
const DASH: &str = "------------------------------------------------------------------------";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyView {
    All,
    CurrentOnly,
}

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub config: Config,
    pub view: StrategyView,
}

pub fn run_cli(app: AppState, args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }

    if args.is_empty() {
        run_interactive_loop(app)
    } else {
        let options = parse_args(args)?;
        run_report(&app, &options);
        Ok(())
    }
}

pub fn run_interactive_loop(app: AppState) -> Result<(), String> {
    println!();
    println!("{SEP}");
    println!("   Medical Care Robot Coordination System - Interactive CLI Interface");
    println!("{SEP}");
    println!("This terminal UI is sharing the same backend state as the frontend/API.");

    loop {
        let options = prompt_cli_options()?;
        run_report(&app, &options);

        if !prompt_yes_no("Run another scheduler report?", true)? {
            println!("Leaving interactive CLI. The HTTP server will keep running until you stop it.");
            break;
        }
    }

    Ok(())
}

pub fn run_report(app: &AppState, options: &CliOptions) {
    let config = options.config.clone();
    app.apply_config(config.clone());
    let state = app.snapshot_state();
    render_report(&state, &config, options.view);
}

fn render_report(state: &SystemState, original_config: &Config, view: StrategyView) {
    let analysis = &state.scheduling_analysis;

    let ew = effective_worker_count(original_config);
    let et = effective_demo_task_count(original_config);
    let displayed_strategies = analysis
        .strategies
        .iter()
        .filter(|strategy| match view {
            StrategyView::All => true,
            StrategyView::CurrentOnly => strategy.scheduler == state.config.scheduler,
        })
        .collect::<Vec<_>>();

    println!();
    println!("{SEP}");
    println!("    Medical Care Robot Coordination System - Scheduling Report");
    println!("{SEP}");
    println!();

    println!("  Scheduler           = {}", scheduler_label(state.config.scheduler));
    println!("  Worker Count        = {ew}");
    println!("  Demo Task Count     = {et}");
    println!(
        "  Work Stealing       = {}",
        bool_label(state.config.use_work_stealing)
    );
    println!(
        "  Stress Preset       = {}",
        bool_label(state.config.use_stress_preset)
    );
    println!(
        "  Strategy View       = {}",
        match view {
            StrategyView::All => "All scheduler results",
            StrategyView::CurrentOnly => "Current scheduler only",
        }
    );
    println!();

    println!("{SEP}");
    println!("                       Test Data (Input Tasks)");
    println!("{SEP}");
    println!(
        "  {:<4} {:<40} {:<8} {:<8} {}",
        "#", "Task Name", "Priority", "Duration", "Zone"
    );
    println!("  {}", "-".repeat(70));
    for task in &analysis.input_tasks {
        let zone = task
            .required_zone_id
            .map(zone_label)
            .unwrap_or_else(|| "-".into());
        println!(
            "  {:<4} {:<40} {:<8} {:<8} {}",
            task.id,
            &task.name,
            priority_label(&task.priority),
            fmt_dur(task.expected_duration_ms),
            zone,
        );
    }
    println!();

    for strategy in &displayed_strategies {
        let is_current = strategy.scheduler == state.config.scheduler;
        let tag = if is_current { "  * Current *" } else { "" };

        println!("{SEP}");
        println!(
            "                    Strategy: {}{tag}",
            scheduler_label(strategy.scheduler)
        );
        println!("{SEP}");

        println!(
            "  Makespan                           = {}",
            fmt_dur(strategy.makespan_ms)
        );
        println!(
            "  Avg Completion                     = {}",
            fmt_dur(strategy.avg_completion_ms)
        );
        println!(
            "  Avg Wait                           = {}",
            fmt_dur(strategy.avg_wait_ms)
        );
        println!(
            "  Urgent Avg Finish                  = {}",
            fmt_dur(strategy.avg_high_priority_completion_ms)
        );

        if matches!(strategy.scheduler, SchedulerKind::Fifo) {
            println!("  Vs FIFO Avg Completion             = baseline");
            println!("  Vs FIFO Urgent                     = baseline");
            println!("  Speedup vs FIFO                    = baseline");
        } else {
            println!(
                "  Vs FIFO Avg Completion             = {}",
                fmt_delta(strategy.avg_completion_improvement_vs_fifo_ms)
            );
            println!(
                "  Vs FIFO Urgent                     = {}",
                fmt_delta(strategy.avg_high_priority_improvement_vs_fifo_ms)
            );
            let sign = if strategy.speedup_vs_fifo_pct >= 0.0 { "+" } else { "" };
            println!(
                "  Speedup vs FIFO                    = {sign}{:.1}%",
                strategy.speedup_vs_fifo_pct
            );
        }

        println!("  {DASH}");
        println!("  Worker Load:");
        for (i, &busy) in strategy.worker_busy_ms.iter().enumerate() {
            println!(
                "    Robot {:<3}                        = {}",
                i + 1,
                fmt_dur(busy)
            );
        }
        println!();
    }

    println!("{SEP}");
    println!("                           Final Summary");
    println!("{SEP}");
    println!(
        "  {:<12} {:<10} {:<12} {:<13} {}",
        "Strategy", "Makespan", "Avg Compl.", "Urgent Fin.", "Speedup vs FIFO"
    );
    println!("  {}", "-".repeat(66));
    for strategy in &displayed_strategies {
        let speedup = if matches!(strategy.scheduler, SchedulerKind::Fifo) {
            "baseline".into()
        } else {
            let sign = if strategy.speedup_vs_fifo_pct >= 0.0 { "+" } else { "" };
            format!("{sign}{:.1}%", strategy.speedup_vs_fifo_pct)
        };
        println!(
            "  {:<12} {:<10} {:<12} {:<13} {}",
            scheduler_label(strategy.scheduler),
            fmt_dur(strategy.makespan_ms),
            fmt_dur(strategy.avg_completion_ms),
            fmt_dur(strategy.avg_high_priority_completion_ms),
            speedup,
        );
    }
    println!("{SEP}");
    println!();
}

fn scheduler_label(kind: SchedulerKind) -> &'static str {
    match kind {
        SchedulerKind::Fifo => "Fifo",
        SchedulerKind::Priority => "Priority",
        SchedulerKind::RoundRobin => "RoundRobin",
        SchedulerKind::Srt => "Srt",
    }
}

fn bool_label(v: bool) -> &'static str {
    if v { "ON" } else { "OFF" }
}

fn zone_label(id: u64) -> String {
    match id {
        1 => "ICU".into(),
        2 => "Ward".into(),
        3 => "OR".into(),
        other => format!("Zone({other})"),
    }
}

fn priority_label(p: &TaskPriority) -> &'static str {
    match p {
        TaskPriority::Low => "Low",
        TaskPriority::Normal => "Normal",
        TaskPriority::High => "High",
    }
}

fn fmt_dur(ms: u64) -> String {
    if ms >= 1000 {
        if ms % 1000 == 0 {
            format!("{} s", ms / 1000)
        } else {
            format!("{:.1} s", ms as f64 / 1000.0)
        }
    } else {
        format!("{ms} ms")
    }
}

fn fmt_delta(ms: i64) -> String {
    let abs = fmt_dur(ms.unsigned_abs());
    if ms > 0 {
        format!("{abs} faster")
    } else if ms < 0 {
        format!("{abs} slower")
    } else {
        "same as FIFO".into()
    }
}

fn prompt_cli_options() -> Result<CliOptions, String> {
    let default = Config::default();

    println!();
    println!("{DASH}");
    println!("Enter CLI options (press Enter to keep the default shown in brackets)");
    println!("{DASH}");

    let scheduler = prompt_scheduler(default.scheduler)?;
    let worker_count = prompt_usize("Number of robots", default.worker_count)?;
    let demo_task_count = prompt_usize("Number of tasks", default.demo_task_count)?;
    let use_work_stealing = prompt_yes_no("Enable work-stealing mode?", default.use_work_stealing)?;
    let use_stress_preset = prompt_yes_no("Enable stress preset?", default.use_stress_preset)?;
    let view = prompt_strategy_view()?;

    Ok(CliOptions {
        config: Config {
            scheduler,
            worker_count,
            demo_task_count,
            use_work_stealing,
            use_stress_preset,
        },
        view,
    })
}

fn prompt_scheduler(default: SchedulerKind) -> Result<SchedulerKind, String> {
    loop {
        println!("Choose scheduler:");
        println!("  1) Fifo");
        println!("  2) Priority");
        println!("  3) RoundRobin");
        println!("  4) Srt");

        let default_choice = match default {
            SchedulerKind::Fifo => "1",
            SchedulerKind::Priority => "2",
            SchedulerKind::RoundRobin => "3",
            SchedulerKind::Srt => "4",
        };
        let input = prompt_line(&format!("Scheduler [{default_choice}]"))?;
        let normalized = if input.trim().is_empty() {
            default_choice.to_string()
        } else {
            input.trim().to_ascii_lowercase()
        };

        let scheduler = match normalized.as_str() {
            "1" | "fifo" => Some(SchedulerKind::Fifo),
            "2" | "priority" => Some(SchedulerKind::Priority),
            "3" | "roundrobin" | "round-robin" | "rr" => Some(SchedulerKind::RoundRobin),
            "4" | "srt" => Some(SchedulerKind::Srt),
            _ => None,
        };

        if let Some(scheduler) = scheduler {
            return Ok(scheduler);
        }

        println!("Please enter 1, 2, 3, 4, fifo, priority, roundrobin, or srt.");
    }
}

fn prompt_strategy_view() -> Result<StrategyView, String> {
    loop {
        println!("How should the report be displayed?");
        println!("  1) All scheduler results");
        println!("  2) Current scheduler only");

        let input = prompt_line("View [1]")?;
        let normalized = if input.trim().is_empty() {
            "1".to_string()
        } else {
            input.trim().to_ascii_lowercase()
        };

        match normalized.as_str() {
            "1" | "all" => return Ok(StrategyView::All),
            "2" | "current" | "current-only" => return Ok(StrategyView::CurrentOnly),
            _ => println!("Please enter 1/all or 2/current."),
        }
    }
}

fn prompt_usize(label: &str, default: usize) -> Result<usize, String> {
    loop {
        let input = prompt_line(&format!("{label} [{default}]"))?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Ok(default);
        }

        match trimmed.parse::<usize>() {
            Ok(value) if value >= 1 => return Ok(value),
            _ => println!("Please enter an integer greater than or equal to 1."),
        }
    }
}

fn prompt_yes_no(label: &str, default: bool) -> Result<bool, String> {
    let suffix = if default { "Y/n" } else { "y/N" };
    loop {
        let input = prompt_line(&format!("{label} [{suffix}]"))?;
        let normalized = input.trim().to_ascii_lowercase();

        if normalized.is_empty() {
            return Ok(default);
        }

        match normalized.as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please answer y/yes or n/no."),
        }
    }
}

fn prompt_line(label: &str) -> Result<String, String> {
    print!("{label}: ");
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to flush stdout: {error}"))?;

    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read input: {error}"))?;
    if bytes == 0 {
        return Err("Standard input closed".into());
    }
    Ok(input)
}

fn parse_args(args: &[String]) -> Result<CliOptions, String> {
    let mut config = Config::default();
    let mut view = StrategyView::All;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scheduler" | "-s" => {
                i += 1;
                let val = args.get(i).ok_or("Missing value for --scheduler")?;
                config.scheduler = match val.to_ascii_lowercase().as_str() {
                    "fifo" => SchedulerKind::Fifo,
                    "priority" => SchedulerKind::Priority,
                    "roundrobin" | "rr" => SchedulerKind::RoundRobin,
                    "srt" => SchedulerKind::Srt,
                    _ => {
                        return Err(format!(
                            "Unknown scheduler: {val}. Use: fifo, priority, roundrobin, srt"
                        ))
                    }
                };
            }
            "--workers" | "-w" => {
                i += 1;
                let val = args.get(i).ok_or("Missing value for --workers")?;
                config.worker_count = val
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid worker count: {val}"))?;
                if config.worker_count < 1 {
                    return Err("Worker count must be >= 1".into());
                }
            }
            "--tasks" | "-t" => {
                i += 1;
                let val = args.get(i).ok_or("Missing value for --tasks")?;
                config.demo_task_count = val
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid task count: {val}"))?;
                if config.demo_task_count < 1 {
                    return Err("Task count must be >= 1".into());
                }
            }
            "--work-stealing" | "--ws" => {
                config.use_work_stealing = true;
            }
            "--stress" => {
                config.use_stress_preset = true;
            }
            "--view" => {
                i += 1;
                let val = args.get(i).ok_or("Missing value for --view")?;
                view = match val.to_ascii_lowercase().as_str() {
                    "all" => StrategyView::All,
                    "current" | "current-only" => StrategyView::CurrentOnly,
                    _ => return Err(format!("Unknown view: {val}. Use: all, current")),
                };
            }
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }
    Ok(CliOptions { config, view })
}

pub fn print_usage() {
    println!(
        r#"Usage: cli [OPTIONS]

Options:
  -s, --scheduler <KIND>  Scheduling algorithm: fifo | priority | roundrobin | srt
                          (default: fifo)
  -w, --workers <N>       Number of worker robots (default: 10)
  -t, --tasks <N>         Number of demo tasks (default: 30)
      --work-stealing     Enable work-stealing + non-blocking zone allocation
      --stress            Use stress test preset (12 workers, 108 tasks)
      --view <MODE>       Report view: all | current (default: all)
  -h, --help              Show this help message

Examples:
  cargo run                         Start server + interactive terminal CLI
  cargo run -- --server-only        Start only the HTTP server
  cargo run --bin cli               Launch interactive terminal mode only
  cargo run --bin cli -- -s priority -w 6 -t 18
  cargo run --bin cli -- --stress --view current"#
    );
}