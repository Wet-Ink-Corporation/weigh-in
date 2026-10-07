//! The source write-back of `decide`, end to end: which lines get the arrow and which are left
//! as they were.

use crate::support::{scan_one, setup, stamped_date, weighin};
use serde_json::json;

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_todo_inside_a_string_is_decided_but_not_annotated() {
    let (_tmp, repo) = setup();
    let source = "const s = \"// TODO: decide later what this is\"; console.log(s);\n";
    std::fs::write(repo.join("src/c.js"), source).unwrap();
    let id = scan_one(&repo, "src/c.js");

    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "keep"], None);

    assert!(
        ok && out.contains("not annotated src/c.js (the comment marker is inside a string)"),
        "{out}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("src/c.js")).unwrap(),
        source
    );
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_line_comment_after_a_closed_block_comment_is_annotated_at_the_end() {
    let (_tmp, repo) = setup();
    std::fs::write(
        repo.join("src/f.c"),
        "/* a */ int x; // TODO: pick a width\n",
    )
    .unwrap();
    let id = scan_one(&repo, "src/f.c");

    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "32"], None);

    assert!(ok && out.contains("annotated src/f.c"), "{out}");
    let after = std::fs::read_to_string(repo.join("src/f.c")).unwrap();
    let today = stamped_date(&after, &id, "32");
    assert_eq!(
        after,
        format!("/* a */ int x; // TODO: pick a width → Weigh-In {id}: decided 32 ({today})\n")
    );
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_question_quoted_from_a_text_file_is_annotated_at_the_end_of_its_line() {
    let (_tmp, repo) = setup();
    std::fs::write(repo.join("notes.txt"), "Should the cache be shared?\n").unwrap();
    let raise = json!([{"op": "raise", "q": "Should the cache be shared?", "door": "two-way",
        "authority": "judgment", "channel": "sweep", "quote": "Should the cache be shared?",
        "source_path": "notes.txt", "source_line": 1}]);
    let (ok, out) = weighin(&repo, "s1", &["batch"], Some(&raise.to_string()));
    assert!(ok, "{out}");
    let id = out
        .lines()
        .find_map(|l| l.strip_prefix("✓ raised "))
        .and_then(|l| l.split_whitespace().next())
        .expect("the raise succeeded")
        .to_owned();

    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "yes"], None);

    assert!(ok && out.contains("annotated notes.txt"), "{out}");
    let after = std::fs::read_to_string(repo.join("notes.txt")).unwrap();
    let today = stamped_date(&after, &id, "yes");
    assert_eq!(
        after,
        format!("Should the cache be shared? → Weigh-In {id}: decided yes ({today})\n")
    );
}
