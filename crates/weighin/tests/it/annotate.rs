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

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_todo_after_a_lifetime_or_a_deref_is_decided_but_not_annotated() {
    for (file, source) in [
        (
            "src/l.rs",
            "fn f(x: impl Fn() + 'static) { panic!(\"TODO: pick a limit\") }\n",
        ),
        ("src/d.rs", "    *limit = todo!(\"pick a cap\");\n"),
    ] {
        let (_tmp, repo) = setup();
        std::fs::write(repo.join(file), source).unwrap();
        let id = scan_one(&repo, file);

        let (ok, out) = weighin(&repo, "s1", &["decide", &id, "4096"], None);

        assert!(
            ok && out.contains(&format!("not annotated {file} (no comment on the line)")),
            "{out}"
        );
        assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), source);
    }
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_comment_marker_that_is_code_is_decided_but_not_annotated() {
    for (file, source) in [
        (
            "src/half.py",
            "half = len(items) // 2 if use_temporary_cache else 0\n",
        ),
        (
            "src/get.sh",
            "curl -s https://jsonplaceholder.typicode.com/todo/items | jq .\n",
        ),
        (
            "src/Dockerfile",
            "RUN curl -fsSL https://example.com/temporary-install.sh | sh\n",
        ),
        ("src/ci.yml", "- run: wget http://host/todo/list.txt\n"),
        ("src/conf.yaml", "url: http://example.com/temporary\n"),
        ("src/count.c", "x = y -- TODO_count;\n"),
        ("src/scale.c", "x = a\n    * TODO_FACTOR;\n"),
        ("src/raw.rs", "let s = r#\"a\"b // TODO \"x\"#;\n"),
        ("src/pick.sh", "echo ' TODO: pick\n"),
        ("src/run", "half = n // 2 if temporary else 0\n"),
    ] {
        let (_tmp, repo) = setup();
        std::fs::write(repo.join(file), source).unwrap();
        let id = scan_one(&repo, file);

        let (ok, out) = weighin(&repo, "s1", &["decide", &id, "X"], None);

        assert!(
            ok && out.contains(&format!("not annotated {file} (")),
            "{file}: {out}"
        );
        assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), source);
    }
}
