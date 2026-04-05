// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::schema::Mode;

const CODE_REVIEW: &str = include_str!("../prompts_md/balthasar_code_review.md");
const DESIGN: &str = include_str!("../prompts_md/balthasar_design.md");
const ANALYSIS: &str = include_str!("../prompts_md/balthasar_analysis.md");

/// Returns the system prompt for Balthasar in the given mode.
pub fn prompt_for_mode(mode: &Mode) -> &'static str {
    match mode {
        Mode::CodeReview => CODE_REVIEW,
        Mode::Design => DESIGN,
        Mode::Analysis => ANALYSIS,
    }
}
