// TARGET: orchestrator.rs
// EXPECT: catch-all arm
#[cfg(test)]
mod early_tests {
    fn t() {}
}

fn f() {
    match outcome {
        A => one(),
        _ => two(),
    }
}

#[cfg(test)]
mod tests {
    fn t() {}
}
