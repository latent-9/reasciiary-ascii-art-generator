use super::*;

fn stage_names(line: &str) -> Vec<String> {
    parse(line)
        .expect("parses")
        .stages
        .into_iter()
        .map(|stage| stage.name)
        .collect()
}

#[test]
fn a_bare_tool_is_one_stage() {
    let command = parse("ascii logo.txt").expect("parses");
    assert_eq!(stage_names("ascii logo.txt"), ["ascii"]);
    assert_eq!(command.stages[0].params.first_positional(), Some("logo.txt"));
    assert_eq!(command.output, None);
}

#[test]
fn pipes_separate_the_source_from_its_filters() {
    assert_eq!(
        stage_names("ascii logo.txt | crt --curve 0.2 | vhs > out.mp4"),
        ["ascii", "crt", "vhs"]
    );
}

#[test]
fn the_path_after_the_arrow_is_the_output() {
    let command = parse("ascii logo.txt > out.mp4").expect("parses");
    assert_eq!(command.output.as_deref(), Some("out.mp4"));
    // The arrow and its path belong to the command, not to the last stage.
    assert_eq!(command.stages[0].params.positional, ["logo.txt"]);
}

#[test]
fn a_flag_takes_the_next_token_unless_that_is_a_flag_too() {
    let command = parse("ascii logo.txt --depth 12 --still --zoom 0.9").expect("parses");
    let params = &command.stages[0].params;
    assert_eq!(params.string("depth"), Some("12"));
    assert_eq!(params.string("zoom"), Some("0.9"));
    // `--still` carries nothing and must not swallow `--zoom`.
    assert!(params.is_set("still"));
    assert_eq!(params.string("still"), None);
}

/// A drawing chosen from a file dialog routinely sits under a path with spaces
/// in it, and losing that is losing the file.
#[test]
fn quotes_hold_a_path_together() {
    let command = parse(r#"ascii "my drawings/a logo.txt" > "out file.gif""#).expect("parses");
    assert_eq!(command.stages[0].params.first_positional(), Some("my drawings/a logo.txt"));
    assert_eq!(command.output.as_deref(), Some("out file.gif"));
}

#[test]
fn typing_numbers_wrong_says_which_flag() {
    let command = parse("ascii logo.txt --depth wide").expect("parses");
    let error = command.stages[0].params.f64("depth", 8.0).expect_err("not a number");
    assert!(error.contains("--depth"), "unhelpful message: {error}");
    assert!(error.contains("wide"), "unhelpful message: {error}");
}

#[test]
fn an_empty_line_is_refused() {
    assert_eq!(parse("").unwrap_err(), "nothing to run");
    assert_eq!(parse("   ").unwrap_err(), "nothing to run");
    // An output with no command is the same complaint, not a pipe one.
    assert_eq!(parse("> out.mp4").unwrap_err(), "nothing to run");
}

#[test]
fn a_malformed_line_says_what_is_wrong() {
    assert!(parse("ascii logo.txt >").unwrap_err().contains("needs a file"));
    assert!(parse("ascii a.txt > one.mp4 two.mp4").unwrap_err().contains("only one file"));
    assert!(parse("ascii a.txt | | crt").unwrap_err().contains("pipe segment is empty"));
    assert!(parse("ascii a.txt --").unwrap_err().contains("not a flag"));
}
