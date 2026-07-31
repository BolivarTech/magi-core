// TARGET: providers/subject.rs
//! A raw string whose contents would wreck the quote walk. Leaving such a line unstripped is the
//! safe direction; cutting it would delete real code from every rule's view.

pub fn sample() -> &'static str {
    r#"{"note": "a \" and a // inside a raw string"}"#
}

pub fn also_fine() -> &'static str {
    "an ordinary literal // with a slash pair" // and a real trailing comment
}
