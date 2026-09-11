//! Extension / filename -> language mapping (cloc-style, common subset).

/// Map a file (by name) to a language label. Unknown text files fall back to
/// `"Text/Other"`. Classification is name-based only; whether the file is text
/// or binary is decided separately in `walk`.
pub fn language_of(file_name: &str) -> &'static str {
    // Special whole-name matches first.
    match file_name {
        "Dockerfile" => return "Dockerfile",
        "Makefile" | "makefile" | "GNUmakefile" => return "Makefile",
        "CMakeLists.txt" => return "CMake",
        "Cargo.toml" | "Cargo.lock" => return "TOML",
        ".gitignore" | ".dockerignore" | ".gitattributes" => return "Ignore/Config",
        _ => {}
    }

    let ext = file_name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let lower = ext.to_ascii_lowercase();
    match lower.as_str() {
        // Systems / compiled
        "rs" => "Rust",
        "go" => "Go",
        "c" | "h" => "C",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "C++",
        "cs" => "C#",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "swift" => "Swift",
        "m" | "mm" => "Objective-C",
        "zig" => "Zig",
        // Scripting
        "py" | "pyi" => "Python",
        "rb" => "Ruby",
        "php" => "PHP",
        "pl" | "pm" => "Perl",
        "lua" => "Lua",
        "sh" | "bash" | "zsh" => "Shell",
        "ps1" => "PowerShell",
        // Web / frontend
        "js" | "cjs" | "mjs" => "JavaScript",
        "jsx" => "JSX",
        "ts" => "TypeScript",
        "tsx" => "TSX",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "scss" | "sass" => "Sass",
        "less" => "Less",
        // Data / config
        "json" => "JSON",
        "jsonl" | "ndjson" => "JSON Lines",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "ini" | "cfg" | "conf" => "INI/Config",
        "xml" => "XML",
        "csv" => "CSV",
        "tsv" => "TSV",
        "proto" => "Protobuf",
        "sql" => "SQL",
        "graphql" | "gql" => "GraphQL",
        // Docs
        "md" | "markdown" => "Markdown",
        "rst" => "reStructuredText",
        "txt" | "text" => "Text",
        "tex" => "TeX",
        "adoc" | "asciidoc" => "AsciiDoc",
        // Build / infra
        "mk" => "Makefile",
        "cmake" => "CMake",
        "gradle" => "Gradle",
        "tf" | "tfvars" => "Terraform",
        "dockerfile" => "Dockerfile",
        // Common binary types (still labelled; tokens are skipped in walk)
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" => "Image",
        "pdf" => "PDF",
        "zip" | "gz" | "tar" | "tgz" | "bz2" | "xz" | "7z" | "rar" => "Archive",
        "woff" | "woff2" | "ttf" | "otf" | "eot" => "Font",
        "mp3" | "wav" | "flac" | "ogg" | "mp4" | "mov" | "avi" | "mkv" => "Media",
        "wasm" | "so" | "dylib" | "dll" | "exe" | "o" | "a" | "bin" => "Binary",
        _ => "Text/Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_extensions() {
        assert_eq!(language_of("main.rs"), "Rust");
        assert_eq!(language_of("SKILL.md"), "Markdown");
        assert_eq!(language_of("data.YAML"), "YAML");
        assert_eq!(language_of("Dockerfile"), "Dockerfile");
        assert_eq!(language_of("Cargo.toml"), "TOML");
        assert_eq!(language_of("mystery.qwerty"), "Text/Other");
        assert_eq!(language_of("logo.png"), "Image");
    }
}
