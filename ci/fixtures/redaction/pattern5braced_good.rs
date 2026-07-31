// TARGET: orchestrator.rs
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
    };
}
fn unrelated() {
    match something_else {
        _ => fine(),
    }
}
