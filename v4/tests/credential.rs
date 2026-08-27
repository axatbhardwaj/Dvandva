use std::sync::{Arc, Barrier};

use dvandva_v4::{claim::Role, credential};

#[test]
fn concurrent_sessions_can_create_the_shared_credential_root() {
    const SESSION_COUNT: usize = 32;

    let root = tempfile::tempdir().unwrap();
    let credentials = Arc::new(root.path().join("credentials"));
    let barrier = Arc::new(Barrier::new(SESSION_COUNT));
    let threads = (0..SESSION_COUNT)
        .map(|index| {
            let credentials = Arc::clone(&credentials);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                credential::prepare(
                    &credentials,
                    &format!("session-{index}"),
                    "run-1",
                    Role::Worker,
                )
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        thread.join().unwrap().unwrap();
    }
}
