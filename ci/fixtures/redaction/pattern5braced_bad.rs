// TARGET: orchestrator.rs
// EXPECT: catch-all arm
fn f() {
    let (kind, detail) = match outcome {
        ModelOutcome::Success(output) => {
            commit(output);
            (RotationKind::Schema, String::new())
        }
        ModelOutcome::Schema(detail) => {
            condemn();
            (RotationKind::Schema, detail)
        }
        _ => (RotationKind::Transport, String::new()),
    };
}
