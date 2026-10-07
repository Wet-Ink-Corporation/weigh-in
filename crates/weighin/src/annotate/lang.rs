//! The comment syntax of a source file, from its name: which line comment openers and which
//! block comments it has. The TODO scan reads every marker in every file, so a line it raised
//! from may hold a marker that is code in that file's language (Python's `a // b`); the
//! write-back trusts only the language's own.

use std::path::Path;

/// A block comment: its opener, its closer and the text a note inside it must not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Block {
    /// `/*` or `<!--`.
    pub(super) open: &'static str,
    /// `*/` or `-->`.
    pub(super) close: &'static str,
    /// `/*`, which opens a nested comment in Rust, Swift and Kotlin; or `--`, which XML does not
    /// allow inside `<!-- -->`.
    pub(super) note_forbids: &'static str,
}

/// `/* … */`.
const STAR: Block = Block {
    open: "/*",
    close: "*/",
    note_forbids: "/*",
};

/// `<!-- … -->`.
const HTML: Block = Block {
    open: "<!--",
    close: "-->",
    note_forbids: "--",
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

    /// Whether `trimmed`, a line without its surrounding whitespace, starts with a line comment
    /// opener of this language.
    pub(super) fn opens_line_comment(self, trimmed: &str) -> bool {
        let opens = |opener: &str, refused: &[char]| {
            trimmed
                .strip_prefix(opener)
                .is_some_and(|rest| !rest.starts_with(refused))
        };
        match self {
            Self::CFamily => opens("//", &[]),
            Self::Css | Self::Markup => false,
            Self::HashSlash => opens("//", &[]) || opens("#", &['[']),
            Self::Hash => opens("#", &[]),
            Self::Sql => opens("--", &[]),
            Self::Dashes => opens("--", &['[']),
            Self::Basic => opens("'", &[]) || opens_rem(trimmed),
            Self::Batch => opens_rem(trimmed),
            Self::Unknown => {
                ["//", "#", "--"]
                    .into_iter()
                    .any(|opener| opens_before_space(trimmed, opener))
                    || trimmed.starts_with("REM ")
            }
        }
    }

    /// Whether a single-quoted string in this language may run over several lines, so a `'`
    /// on a comment line could close one opened above: shell and the other `#` languages, SQL,
    /// and a file whose language is not known.
    pub(super) fn single_quotes_span_lines(self) -> bool {
        matches!(self, Self::Hash | Self::Sql | Self::Unknown)
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

/// Whether `trimmed` starts with `REM ` in any case, as Visual Basic and batch files read it.
fn opens_rem(trimmed: &str) -> bool {
    trimmed
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("REM "))
}

/// Whether `trimmed` starts with `opener` followed by whitespace or the line's end, as a file
/// of unknown language needs (`#define` is not a comment there).
fn opens_before_space(trimmed: &str, opener: &str) -> bool {
    trimmed
        .strip_prefix(opener)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// The language of a file known by its whole (lowercased) name.
fn by_name(name: &str) -> Option<Lang> {
    let hash = matches!(
        name,
        "dockerfile"
            | "containerfile"
            | "makefile"
            | "gnumakefile"
            | "gemfile"
            | "rakefile"
            | "vagrantfile"
            | "procfile"
            | "justfile"
            | ".gitignore"
            | ".gitattributes"
            | ".dockerignore"
            | ".editorconfig"
            | ".env"
            | ".bashrc"
            | ".zshrc"
            | ".profile"
    ) || name.starts_with("dockerfile.")
        || name.starts_with(".env.");
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
        .map(|(lang, line)| lang.opens_line_comment(line))
        .collect();

        assert_eq!(
            opens,
            [
                true, false, true, false, false, false, true, false, true, false
            ]
        );
    }
}
