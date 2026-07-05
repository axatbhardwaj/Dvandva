//! `lint run3-dynamic-agents` — dynamic-agent documentation contract swept
//! across the Dvandva doc/skill/agent surface.
//!
//! No shell-script wording appears in this contract, so the invariants port
//! byte-identical; only `ROOT_DIR` derivation changes to the repo-root arg.

use std::path::Path;

use regex::Regex;

use crate::lint::{
    gather_surface, list_md, resolve_root, surface_contains, surface_matches, Report,
    MODEL_POLICY_CLAUDE_MAPPING, MODEL_POLICY_CODEX_MAPPING, MODEL_POLICY_CODEX_XHIGH,
    MODEL_POLICY_OPUS_ROUTING, MODEL_POLICY_SONNET_ROUTING, MODEL_POLICY_STALE_OPUS_ROUTING,
    MODEL_POLICY_STALE_SONNET_ROUTING,
};

const SURFACE: &[&str] = &[
    "README.md",
    "product.md",
    "docs/protocol",
    "docs/workflows",
    "plugins/dvandva/agents",
    "plugins/dvandva/commands",
    "plugins/dvandva/references",
    "plugins/dvandva/skills",
];

fn content_matches(content: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .map(|re| content.lines().any(|line| re.is_match(line)))
        .unwrap_or(false)
}

/// Build the run3 dynamic-agent findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();
    let surface = gather_surface(root, SURFACE);

    r.add(
        surface_contains(&surface, "agent_instances"),
        "surface names Run 3 agent_instances",
    );
    r.add(
        surface_matches(
            &surface,
            "seed roster|static roster[^[:alnum:]]+as seed|static roster.*seed|seed.*static roster",
        ),
        "surface treats the roster as a seed/static roster",
    );
    r.add(
        surface_matches(
            &surface,
            "run-scoped.*dynamic (agents|agent|instances|instance)|dynamic (agents|agent|instances|instance).*run-scoped",
        ),
        "surface documents run-scoped dynamic agents or instances",
    );
    r.add(
        surface_matches(
            &surface,
            "explicit (Codex )?subagent handle closure|subagent handle closure|explicit closure|every generated handle must be explicitly closed|close[sd]?.*subagent handle|close[sd]?.*generated handle",
        ),
        "surface requires explicit subagent handle closure",
    );
    r.add(
        surface_matches(
            &surface,
            "write-path disjoint|write path disjoint|dynamic write-path|conflict_group|serializ(e|ation).*conflict_group",
        ),
        "surface documents write-path disjointness or conflict_group serialization",
    );
    r.add(
        surface_matches(
            &surface,
            "no daemon|There is no daemon|without adding a daemon",
        ),
        "surface rejects a runtime daemon",
    );
    r.add(
        surface_matches(
            &surface,
            "no mailbox|without adding a daemon, mailbox, or central runtime process|mailbox, or central runtime process",
        ),
        "surface rejects a runtime mailbox",
    );
    r.add(
        surface_matches(
            &surface,
            "hidden scheduler|hidden central process|hidden process that owns the control loop",
        ),
        "surface rejects a hidden scheduler or central owner",
    );
    r.add(
        surface_contains(&surface, MODEL_POLICY_CLAUDE_MAPPING),
        "surface documents Anthropic opus/sonnet model-class mapping",
    );
    r.add(
        surface_contains(&surface, MODEL_POLICY_CODEX_MAPPING),
        "surface documents Codex gpt-5.5/gpt-5.4 model-class mapping",
    );
    r.add(
        surface_contains(&surface, MODEL_POLICY_CODEX_XHIGH),
        "surface documents Codex xhigh effort guidance",
    );
    r.add(
        surface_contains(&surface, MODEL_POLICY_OPUS_ROUTING),
        "surface documents opus workload routing",
    );
    r.add(
        surface_contains(&surface, MODEL_POLICY_SONNET_ROUTING),
        "surface documents sonnet workload routing",
    );
    r.add(
        !surface_contains(&surface, MODEL_POLICY_STALE_OPUS_ROUTING),
        "surface avoids stale broad opus workload wording",
    );
    r.add(
        !surface_contains(&surface, MODEL_POLICY_STALE_SONNET_ROUTING),
        "surface avoids stale broad sonnet workload wording",
    );
    r.add(
        surface_matches(
            &surface,
            "generated agents?.*(do not|must not|never).*(own|set|mutate).*(assignee|active_roles|transitions)|(assignee|active_roles|transitions).*(do not|must not|never).*(belong to|owned by).*(generated agents?)",
        ),
        "surface says generated agents do not own assignee, active_roles, or transitions",
    );

    // Seed agent-instance files declare the generated-instance contract.
    let seed_re = Regex::new(
        "seed roster.*dynamic agent-instance seed|dynamic agent-instance seed|same seed agent contract",
    )
    .unwrap();
    for path in list_md(root, "plugins/dvandva/agents") {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !content.lines().any(|line| seed_re.is_match(line)) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("agent")
            .to_string();
        r.add(
            content.contains("agent_instances"),
            format!("{name} names agent_instances"),
        );
        r.add(
            content.contains("work_item_ids"),
            format!("{name} binds work_item_ids"),
        );
        r.add(
            content_matches(
                &content,
                "same seed agent contract|same agent contract as its seed agent",
            ),
            format!("{name} requires generated briefs to satisfy the seed contract"),
        );
        r.add(
            content_matches(&content, "explicit closure|closed generated instance"),
            format!("{name} requires explicit closure"),
        );
        r.add(
            content_matches(
                &content,
                "never own the baton|generated agents? never own.*assignee|generated instances never own.*assignee|never own `assignee`",
            ),
            format!("{name} keeps generated agents out of baton ownership"),
        );
        r.add(
            content_matches(
                &content,
                "dynamic write-path disjointness|write-path disjointness",
            ),
            format!("{name} documents write-path disjointness"),
        );
        r.add(
            content_matches(&content, "planned.*running|running.*planned|live"),
            format!("{name} documents live instance collision scope"),
        );
        r.add(
            content_matches(
                &content,
                "conflict_group.*depends_on|depends_on.*conflict_group",
            ),
            format!("{name} documents serialized conflict-group overlap"),
        );
    }

    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}
