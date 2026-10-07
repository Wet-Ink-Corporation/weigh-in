//! The source write-back of `decide`, end to end: a comment that is the whole line gets the
//! arrow, anything else is left as it was.

use crate::support::{scan_one, setup, stamped_date, weighin};
use serde_json::json;

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_line_comment_todo_is_annotated_at_the_end() {
    let (_tmp, repo) = setup();
    std::fs::write(
        repo.join("src/cap.py"),
        "def f():\n    # TODO: pick a cap\n    return 1\n",
    )
    .unwrap();
    let id = scan_one(&repo, "src/cap.py");

    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "4096"], None);

    assert!(ok && out.contains("annotated src/cap.py"), "{out}");
    let after = std::fs::read_to_string(repo.join("src/cap.py")).unwrap();
    let today = stamped_date(&after, &id, "4096");
    assert_eq!(
        after,
        format!(
            "def f():\n    # TODO: pick a cap → Weigh-In {id}: decided 4096 ({today})\n    return 1\n"
        )
    );
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_block_comment_alone_on_its_line_is_annotated_inside() {
    let (_tmp, repo) = setup();
    std::fs::write(repo.join("src/x.c"), "/* TODO: pick a cap */\n").unwrap();
    let id = scan_one(&repo, "src/x.c");

    let (ok, out) = weighin(&repo, "s1", &["decide", &id, "y"], None);

    assert!(ok && out.contains("annotated src/x.c"), "{out}");
    let after = std::fs::read_to_string(repo.join("src/x.c")).unwrap();
    let today = stamped_date(&after, &id, "y");
    assert_eq!(
        after,
        format!("/* TODO: pick a cap → Weigh-In {id}: decided y ({today}) */\n")
    );
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_trailing_comment_after_code_is_decided_but_not_annotated() {
    for (file, source) in [
        ("src/t.py", "x = 1  # TODO: pick a start\n"),
        ("src/f.c", "/* a */ int x; // TODO: pick a width\n"),
        (
            "src/c.js",
            "const s = \"// TODO: decide later what this is\"; console.log(s);\n",
        ),
    ] {
        let (_tmp, repo) = setup();
        std::fs::write(repo.join(file), source).unwrap();
        let id = scan_one(&repo, file);

        let (ok, out) = weighin(&repo, "s1", &["decide", &id, "1"], None);

        assert!(
            ok && out.contains(&format!("decided {id} → 1"))
                && out.contains(&format!("not annotated {file} (")),
            "{file}: {out}"
        );
        assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), source);
    }
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

/// Source lines whose marker is code, inside a string or in a comment that is not the whole
/// line, with the reason the receipt gives for leaving each alone; among them every line reviews
/// found (a triple quote, a regex with a quote, a lifetime, a digit separator, a verbatim string,
/// PHP's `?>` and JSX text).
const LEFT_ALONE: &[(&str, &str, &str)] = &[
    (
        "src/l.rs",
        "fn f(x: impl Fn() + 'static) { panic!(\"TODO: pick a limit\") }\n",
        "the comment is not the whole line",
    ),
    (
        "src/d.rs",
        "    *limit = todo!(\"pick a cap\");\n",
        "the comment is not the whole line",
    ),
    (
        "src/half.py",
        "half = len(items) // 2 if use_temporary_cache else 0\n",
        "the comment is not the whole line",
    ),
    (
        "src/get.sh",
        "curl -s https://jsonplaceholder.typicode.com/todo/items | jq .\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.py",
        "help = '''Don't use # TODO: explain'''\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.js",
        "s.replace(/'/g, '// TODO: pick a separator')\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.rs",
        "let v: &'a str = pick(\"' // TODO: decide\", '\"');\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.cpp",
        "int x = 1'000; const char* s = \"' // TODO: q\"; char c = '\"';\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.cs",
        "var p = @\"C:\\\"; Log(\"// TODO: x\" + '\"');\n",
        "the comment is not the whole line",
    ),
    (
        "src/a.php",
        "// TODO: pick ?> <p>\n",
        "code follows the comment",
    ),
    (
        "src/a.jsx",
        "  return <p>// TODO: x</p>;\n",
        "the comment is not the whole line",
    ),
    (
        "src/s.c",
        "#define X 1 // TODO: q9 word9 \\\n",
        "the comment is not the whole line",
    ),
    (
        "src/t.c",
        "// TODO: q9 word9 \\\n",
        "the comment runs on to the next line",
    ),
    (
        "src/sep.js",
        "// TODO: pick \u{2028} let x=1;\n",
        "the comment runs on to the next line",
    ),
    (
        "src/w.c",
        "/*\n// TODO: pick a width */\nint x;\n",
        "the line may close a string or comment opened above",
    ),
    (
        "src/doc.py",
        "def f():\n    \"\"\"Doc.\n    # TODO: pick a cap\"\"\"\n",
        "the line may close a string or comment opened above",
    ),
    (
        "src/t.js",
        "const s = `\n// TODO: pick a tag`;\n",
        "the line may close a string or comment opened above",
    ),
    (
        "src/e.sh",
        "echo 'a\n# TODO: pick a word'\n",
        "the line may close a string or comment opened above",
    ),
];

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn an_option_that_could_close_a_string_is_decided_but_not_annotated_in_code() {
    for option in ["x \"\"\" y", r"C:\N", r"use \x1"] {
        let (_tmp, repo) = setup();
        let source = "s = \"\"\"\n# TODO: pick a cap\n\"\"\"\n";
        std::fs::write(repo.join("src/s.py"), source).unwrap();
        let id = scan_one(&repo, "src/s.py");

        let (ok, out) = weighin(&repo, "s1", &["decide", &id, option], None);

        assert!(
            ok && out.contains(
                "not annotated src/s.py (the option holds a quote, bracket or backslash that could end or change a string around the comment)"
            ),
            "{option}: {out}"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("src/s.py")).unwrap(),
            source,
            "{option}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "spawns the weighin binary and git")]
fn a_marker_that_is_code_or_inside_a_string_is_decided_but_not_annotated() {
    for &(file, source, reason) in LEFT_ALONE {
        let (_tmp, repo) = setup();
        std::fs::write(repo.join(file), source).unwrap();
        let id = scan_one(&repo, file);

        let (ok, out) = weighin(&repo, "s1", &["decide", &id, "X"], None);

        assert!(
            ok && out.contains(&format!("not annotated {file} ({reason})")),
            "{file}: {out}"
        );
        assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), source);
    }
}
