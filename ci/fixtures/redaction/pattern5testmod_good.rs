// TARGET: orchestrator.rs
fn f() {
    match outcome {
        A => one(),
        B => two(),
    }
}

#[cfg(test)]
mod input_threshold_tests {
    #[test]
    fn an_oversized_response_routes_to_its_own_outcome() {
        match outcome {
            ModelOutcome::OversizedResponse { limit } => assert_eq!(limit, 4096),
            _ => panic!("expected OversizedResponse"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn another() {
        match outcome {
            _ => panic!("also legitimate"),
        }
    }
}
