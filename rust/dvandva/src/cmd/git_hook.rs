//! Git-hook entry (argv[0] or `dvandva git-hook <name>`) — Wave B4 port target.

pub fn run(name: &str, args: &[String]) -> i32 {
    let _ = args;
    eprintln!("dvandva git-hook {name}: not implemented");
    2
}
