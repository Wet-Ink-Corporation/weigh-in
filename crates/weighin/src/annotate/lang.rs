//! The comment syntax of a source file, from its name: which of the markers `weighin-core`'s TODO
//! scan recognises open a comment in it. The scan reads every marker in every file, so a line it
//! raised from may hold a marker that is code in that file's language (Python's `a // b`, a
//! shell URL); the write-back reads only the markers the language has.

use std::path::Path;

/// A comment marker the TODO scan recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Marker {
    /// `<!-- … -->`.
    Html,
    /// `/* … */`, and a block comment's middle line that starts with `*`.
    Star,
    /// `//`.
    Slash,
    /// `--`.
    Dashes,
    /// `#`.
    Hash,
    /// `REM `.
    Rem,
    /// Visual Basic's standalone `'`.
    Quote,
}

/// The comment syntax of a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    /// `//` and `/* */`: C, C++, C#, Java, JavaScript, TypeScript, Go, Swift, Kotlin and the like.
    CFamily,
    /// Rust: the C family's comments, and raw strings (`r#"…"#`).
    Rust,
    /// JSX and TSX: the C family's comments, in a file where element text (`<p>// x</p>`) may
    /// hold a `//` that is no comment.
    Jsx,
    /// `/* */` only: CSS.
    Css,
    /// `#`, `//` and `/* */`: PHP, Terraform and HCL.
    HashSlash,
    /// `#`: Python, shell, Ruby, Perl, R, YAML, TOML, Dockerfiles, Makefiles and the like.
    Hash,
    /// `--` and `/* */`: SQL.
    Sql,
    /// `--`: Lua, Haskell, Elm, Ada, VHDL.
    Dashes,
    /// `<!-- -->`: HTML, XML, SVG.
    Markup,
    /// `'` and `REM `: Visual Basic.
    Basic,
    /// `REM `: Windows batch files.
    Batch,
    /// A file whose language is not known: every marker counts, and one that may be code is
    /// trusted only where nothing else could stand (at the start of the line).
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

    /// Whether `marker` opens a comment in this language.
    pub(super) fn allows(self, marker: Marker) -> bool {
        use Marker::{Dashes, Hash, Html, Quote, Rem, Slash, Star};
        match self {
            Self::CFamily | Self::Rust | Self::Jsx => matches!(marker, Slash | Star),
            Self::Css => marker == Star,
            Self::HashSlash => matches!(marker, Hash | Slash | Star),
            Self::Hash => marker == Hash,
            Self::Sql => matches!(marker, Dashes | Star),
            Self::Dashes => marker == Dashes,
            Self::Markup => marker == Html,
            Self::Basic => matches!(marker, Quote | Rem),
            Self::Batch => marker == Rem,
            Self::Unknown => true,
        }
    }

    /// Whether the language is known, so a marker it allows is a comment wherever it stands
    /// outside a string.
    pub(super) fn known(self) -> bool {
        self != Self::Unknown
    }
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
        "rs" => Lang::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "m" | "mm" | "cs"
        | "java" | "js" | "mjs" | "cjs" | "ts" | "mts" | "cts" | "go" | "swift" | "kt" | "kts"
        | "scala" | "sc" | "dart" | "groovy" | "gradle" | "proto" | "zig" | "sol" | "scss"
        | "less" | "jsonc" | "json5" => Lang::CFamily,
        "jsx" | "tsx" => Lang::Jsx,
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
                ("src/main.rs", Lang::Rust),
                ("web/app.TS", Lang::CFamily),
                ("web/App.tsx", Lang::Jsx),
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
    fn a_known_language_allows_only_its_own_markers() {
        assert!(Lang::Hash.allows(Marker::Hash) && !Lang::Hash.allows(Marker::Slash));
        assert!(Lang::Rust.allows(Marker::Slash) && !Lang::Rust.allows(Marker::Quote));
        assert!(Lang::Basic.allows(Marker::Quote) && !Lang::Basic.allows(Marker::Hash));
        assert!(Lang::Unknown.allows(Marker::Quote) && !Lang::Unknown.known());
    }
}
