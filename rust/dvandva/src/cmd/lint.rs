//! CLI dispatch for the `dvandva lint <target>` family.

const USAGE: &str = "Usage: dvandva lint <artifacts|skills|protocol-phase1|skill-phase3|phase4-research|run3-dynamic-agents|run4-path-gates|run4-standalone-agents> [args...]";

pub fn run(args: &[String]) -> i32 {
    let Some((target, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return 2;
    };
    match target.as_str() {
        "artifacts" => dvandva::lint::artifacts::run(rest),
        "skills" => dvandva::lint::skills::run(rest),
        "protocol-phase1" => dvandva::lint::protocol_phase1::run(rest),
        "skill-phase3" => dvandva::lint::skill_phase3::run(rest),
        "phase4-research" => dvandva::lint::phase4_research::run(rest),
        "run3-dynamic-agents" => dvandva::lint::run3_dynamic_agents::run(rest),
        "run4-path-gates" => dvandva::lint::run4_path_gates::run(rest),
        "run4-standalone-agents" => dvandva::lint::run4_standalone_agents::run(rest),
        other => {
            eprintln!("dvandva lint: unknown target '{other}'");
            eprintln!("{USAGE}");
            2
        }
    }
}
