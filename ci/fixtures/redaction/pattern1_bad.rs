let a = format!("failed: {e}");
let b = err.to_string();
tracing::warn!(cause = %error, "boom");
tracing::debug!(cause = ?e, "boom");
