//! `trellis-sim [seeds] [agents] [steps]`: runs seeds 0..seeds and reports each.

fn main() {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let seeds = args.first().copied().unwrap_or(50);
    let agents = args.get(1).copied().unwrap_or(30) as usize;
    let steps = args.get(2).copied().unwrap_or(5000) as usize;
    let mut failed = 0;
    for seed in 0..seeds {
        let config = trellis_sim::SimConfig {
            seed,
            agents,
            steps,
            cas_fault: 0.15,
            receipt_fault: 0.05,
        };
        match trellis_sim::run(&config) {
            Ok(r) => println!(
                "seed {seed}: ok landed={} conflicted={} rejected={} unchanged={} errors={} tasks_done={} notifications={} log={}",
                r.landed,
                r.conflicted,
                r.rejected,
                r.unchanged,
                r.errors,
                r.tasks_done,
                r.notifications,
                r.log_entries
            ),
            Err(e) => {
                failed += 1;
                println!("seed {seed}: FAILED {e}");
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
