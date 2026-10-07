//! The comment syntax of a source file, from its name: which line comment openers and which
//! block comments it has. The TODO scan reads every marker in every file, so a line it raised
//! from may hold a marker that is code in that file's language (Python's `a // b`); the
//! write-back trusts only the language's own.

use std::path::Path;

/// A block comment: its opener, its closer and any text an option written inside it must not
/// hold beyond what every code comment refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Block {
    /// `/*` or `<!--`.
    pub(super) open: &'static str,
    /// `*/` or `-->`.
    pub(super) close: &'static str,
    /// `--`, which XML does not allow inside `<!-- -->`.
    pub(super) option_forbids: Option<&'static str>,
}

/// `/* … */`.
const STAR: Block = Block {
    open: "/*",
    close: "*/",
    option_forbids: None,
};

/// `<!-- … -->`.
const HTML: Block = Block {
    open: "<!--",
    close: "-->",
    option_forbids: Some("--"),
};

/// The comment syntax of a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    /// `//` and `/* */`: C, C++, C#, Java, JavaScript, TypeScript, Rust, Go, Swift and the like.
    CFamily,
    /// `/* */` only: CSS.
    Css,
    /// `#` (not PHP's `#[` attribute), `//` and `/* */`: PHP, Terraform and HCL.
    HashSlash,
    /// `#`: Python, shell, Ruby, Perl, R, YAML, TOML, Dockerfiles, Makefiles and the like.
    Hash,
    /// `--` and `/* */`: SQL.
    Sql,
    /// `--` (not Lua's `--[[` block): Lua, Haskell, Elm, Ada, VHDL.
    Dashes,
    /// `<!-- -->`: HTML, XML, SVG.
    Markup,
    /// `'` and `REM ` (in any case): Visual Basic.
    Basic,
    /// `REM ` (in any case): Windows batch files.
    Batch,
    /// A file whose language is not known: `//`, `#`, `--` and `REM ` before whitespace, and
    /// both block comments.
    Unknown,
}

impl Lang {
    /// The language of the file at `rel`, from its name, or else its extension.
    pub(crate) fn of(rel: &str) -> Self {
        let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) else {
            return Self::Unknown;
        };
        let name = name.to_ascii_lowercase();
        by_name(&name).unwrap_or_else(|| {
            Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .map_or(Self::Unknown, by_extension)
        })
    }

    /// The text after the line comment opener `trimmed`, a line without its surrounding
    /// whitespace, starts with in this language; `None` when it starts with none. A `#!` shebang
    /// is not one: its text is the interpreter's arguments.
    pub(super) fn line_comment_body(self, trimmed: &str) -> Option<&str> {
        let opens = |opener: &str, refused: &[char]| {
            trimmed
                .strip_prefix(opener)
                .filter(|rest| !rest.starts_with(refused))
        };
        match self {
            Self::CFamily => opens("//", &[]),
            Self::Css | Self::Markup => None,
            Self::HashSlash => opens("//", &[]).or_else(|| opens("#", &['[', '!'])),
            Self::Hash => opens("#", &['!']),
            Self::Sql => opens("--", &[]),
            Self::Dashes => opens("--", &['[']),
            Self::Basic => opens("'", &[]).or_else(|| after_rem(trimmed)),
            Self::Batch => after_rem(trimmed),
            Self::Unknown => ["//", "#", "--", "REM"]
                .into_iter()
                .find_map(|opener| after_before_space(trimmed, opener)),
        }
    }

    /// The characters that, besides the quotes and brackets of every language, could close a
    /// string opened on an earlier line: `'` wherever a single-quoted string may run on (all but
    /// Visual Basic and batch files), `/` and `!` where Perl and Ruby take them as a string's
    /// own delimiter, and `$` where Postgres's `$tag$` quotes do.
    pub(super) fn closers(self) -> &'static [char] {
        match self {
            Self::Basic | Self::Batch => &[],
            Self::CFamily | Self::Css | Self::HashSlash | Self::Dashes | Self::Markup => &['\''],
            Self::Hash => &['\'', '/', '!'],
            Self::Sql => &['\'', '$'],
            Self::Unknown => &['\'', '/', '!', '$'],
        }
    }

    /// Whether a `%` in a comment may still be expanded: batch files read `%` before `REM`, so
    /// `rem %~` is an error; a file whose language is not known may be one.
    pub(super) fn expands_percent(self) -> bool {
        matches!(self, Self::Batch | Self::Unknown)
    }

    /// The block comments of this language.
    pub(super) fn blocks(self) -> &'static [Block] {
        match self {
            Self::CFamily | Self::Css | Self::HashSlash | Self::Sql => &[STAR],
            Self::Markup => &[HTML],
            Self::Unknown => &[STAR, HTML],
            Self::Hash | Self::Dashes | Self::Basic | Self::Batch => &[],
        }
    }
}

/// The text after `REM ` when `trimmed` starts with it in any case, as Visual Basic and batch
/// files read it.
fn after_rem(trimmed: &str) -> Option<&str> {
    trimmed
        .get(..4)
        .filter(|head| head.eq_ignore_ascii_case("REM "))
        .and_then(|_| trimmed.get(4..))
}

/// The text after `opener` when `trimmed` starts with it followed by whitespace or the line's
/// end, as a file of unknown language needs (`#define` is not a comment there).
fn after_before_space<'a>(trimmed: &'a str, opener: &str) -> Option<&'a str> {
    trimmed
        .strip_prefix(opener)
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// The (lowercased) names of files read with `#` comments, whatever their extension.
const HASH_NAMES: [&str; 17] = [
    "dockerfile",
    "containerfile",
    "makefile",
    "gnumakefile",
    "gemfile",
    "rakefile",
    "vagrantfile",
    "procfile",
    "justfile",
    ".gitignore",
    ".gitattributes",
    ".dockerignore",
    ".editorconfig",
    ".env",
    ".bashrc",
    ".zshrc",
    ".profile",
];

/// The language of a file known by its whole (lowercased) name.
fn by_name(name: &str) -> Option<Lang> {
    let hash =
        HASH_NAMES.contains(&name) || name.starts_with("dockerfile.") || name.starts_with(".env.");
    hash.then_some(Lang::Hash)
}

/// The language of a file with the (lowercased) extension `ext`.
fn by_extension(ext: &str) -> Lang {
    match ext {
        "rs" | "c" | "h" | "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "m" | "mm"
        | "cs" | "java" | "js" | "mjs" | "cjs" | "ts" | "mts" | "cts" | "go" | "swift" | "kt"
        | "kts" | "scala" | "sc" | "dart" | "groovy" | "gradle" | "proto" | "zig" | "sol"
        | "scss" | "less" | "jsonc" | "json5" | "jsx" | "tsx" => Lang::CFamily,
        "css" => Lang::Css,
        "php" | "tf" | "tfvars" | "hcl" => Lang::HashSlash,
        "py" | "pyi" | "pyw" | "sh" | "bash" | "zsh" | "ksh" | "fish" | "rb" | "pl" | "pm"
        | "r" | "yaml" | "yml" | "toml" | "cfg" | "conf" | "mk" | "cmake" | "nix" | "ps1"
        | "psm1" | "ex" | "exs" | "jl" | "awk" | "tcl" | "coffee" | "cr" | "nim" | "gd"
        | "dockerfile" | "containerfile" => Lang::Hash,
        "sql" => Lang::Sql,
        "lua" | "hs" | "elm" | "ada" | "adb" | "ads" | "vhd" | "vhdl" | "purs" => Lang::Dashes,
        "html" | "htm" | "xhtml" | "xml" | "svg" | "xsl" | "xslt" | "plist" => Lang::Markup,
        "vb" | "vbs" | "vba" | "bas" | "cls" | "frm" => Lang::Basic,
        "bat" | "cmd" => Lang::Batch,
        _ => Lang::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_by_its_name_or_else_its_extension() {
        let read: Vec<_> = [
            "src/main.rs",
            "web/app.TS",
            "web/App.tsx",
            "style.css",
            "infra/main.tf",
            "tools/run.py",
            "Dockerfile",
            "docker/Dockerfile.dev",
            ".github/ci.yml",
            "db/q.sql",
            "init.lua",
            "page.html",
            "macro.bas",
            "build.cmd",
            "scripts/run",
        ]
        .into_iter()
        .map(|p| (p, Lang::of(p)))
        .collect();

        assert_eq!(
            read,
            [
                ("src/main.rs", Lang::CFamily),
                ("web/app.TS", Lang::CFamily),
                ("web/App.tsx", Lang::CFamily),
                ("style.css", Lang::Css),
                ("infra/main.tf", Lang::HashSlash),
                ("tools/run.py", Lang::Hash),
                ("Dockerfile", Lang::Hash),
                ("docker/Dockerfile.dev", Lang::Hash),
                (".github/ci.yml", Lang::Hash),
                ("db/q.sql", Lang::Sql),
                ("init.lua", Lang::Dashes),
                ("page.html", Lang::Markup),
                ("macro.bas", Lang::Basic),
                ("build.cmd", Lang::Batch),
                ("scripts/run", Lang::Unknown),
            ]
        );
    }

    #[test]
    fn a_language_opens_a_line_comment_only_with_its_own_openers() {
        let opens: Vec<_> = [
            (Lang::Hash, "# TODO"),
            (Lang::Hash, "#!/bin/sh # TODO"),
            (Lang::HashSlash, "#!/usr/bin/php # TODO"),
            (Lang::Unknown, "#!/bin/sh # TODO"),
            (Lang::Hash, "// TODO"),
            (Lang::CFamily, "// TODO"),
            (Lang::CFamily, "# TODO"),
            (Lang::HashSlash, "#[Route] TODO"),
            (Lang::Dashes, "--[[ TODO ]]"),
            (Lang::Basic, "' TODO"),
            (Lang::Unknown, "#define TODO"),
            (Lang::Unknown, "REM TODO"),
            (Lang::Unknown, "' TODO"),
        ]
        .into_iter()
        .map(|(lang, line)| lang.line_comment_body(line).is_some())
        .collect();

        assert_eq!(
            opens,
            [
                true, false, false, false, false, true, false, false, false, true, false, true,
                false
            ]
        );
    }
}
