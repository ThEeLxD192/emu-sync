//! The integration tests of each crate are one binary, `tests/integration/main.rs`, whose `mod`
//! declarations decide which test files compile. A file left out of them would silently stop
//! running, and any other test target would be another binary that recompiles the shared
//! helpers; this check rejects both, from the test targets Cargo actually builds.

use std::{
    collections::BTreeSet,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

/// A workspace package and the test targets Cargo builds for it (from `cargo metadata`).
pub struct PackageTests {
    pub name: String,
    pub dir: PathBuf,
    pub test_targets: Vec<TestTarget>,
}

pub struct TestTarget {
    pub name: String,
    pub source_path: PathBuf,
}

pub fn verify(packages: &[PackageTests]) -> Result<(), Box<dyn Error>> {
    let problems = all_problems(packages);
    if problems.is_empty() {
        println!("Integration test layout verified");
        return Ok(());
    }
    for problem in &problems {
        eprintln!("{problem}");
    }
    Err(format!("{} integration test layout problem(s)", problems.len()).into())
}

/// Every problem of every package. A file that cannot be read is one more problem, so it
/// never hides the others.
fn all_problems(packages: &[PackageTests]) -> Vec<String> {
    packages.iter().flat_map(layout_problems).collect()
}

fn layout_problems(package: &PackageTests) -> Vec<String> {
    let integration_dir = package.dir.join("tests").join("integration");
    let main_path = integration_dir.join("main.rs");
    let mut problems = Vec::new();
    let mut report = |problem: String| problems.push(format!("{}: {problem}", package.name));
    let mut builds_integration_binary = false;
    for target in &package.test_targets {
        if target.name == "integration" && target.source_path == main_path {
            builds_integration_binary = true;
        } else {
            report(format!(
                "test target `{}` ({}) is a separate test binary; make it a module of \
                 tests/integration/main.rs",
                target.name,
                target.source_path.display()
            ));
        }
    }
    match file_type(&integration_dir) {
        Ok(Some(kind)) if kind.is_dir() => {}
        Ok(_) => return problems,
        Err(error) => {
            report(error);
            return problems;
        }
    }
    let mut is_main_missing = false;
    let main_source = match fs::read_to_string(&main_path) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            report(format!("{} is missing", main_path.display()));
            is_main_missing = true;
            None
        }
        Err(error) => {
            report(format!("cannot read {}: {error}", main_path.display()));
            None
        }
    };
    if !is_main_missing && !builds_integration_binary {
        report(format!(
            "Cargo does not build {} as the `integration` test target",
            main_path.display()
        ));
    }
    let (modules, unreadable) = module_names(&integration_dir);
    for error in unreadable {
        report(error);
    }
    // Without a built `main.rs` nothing in the directory compiles, so comparing its modules
    // would only add reports for files that are not the problem.
    let Some(main_source) = main_source.filter(|_| builds_integration_binary) else {
        return problems;
    };
    let declared = declared_modules(&main_source);
    for module in modules {
        if !declared.contains(&module) {
            report(format!(
                "{} does not declare `mod {module};`, so that test file never compiles",
                main_path.display()
            ));
        }
    }
    problems
}

/// The type of what `path` points to, `None` if nothing is there. Unlike `Path::is_dir`, any
/// other error (such as a denied permission) is reported instead of read as "not there".
fn file_type(path: &Path) -> Result<Option<fs::FileType>, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// Modules a directory can hold: `<name>.rs` files other than `main.rs`, and `<name>/mod.rs`,
/// plus the entries that could not be read, which do not stop the others from being listed.
/// Like Cargo, it skips names starting with a dot (editor lock and backup files).
fn module_names(dir: &Path) -> (BTreeSet<String>, Vec<String>) {
    let mut names = BTreeSet::new();
    let mut unreadable = Vec::new();
    let cannot_list = |error: io::Error| format!("cannot list {}: {error}", dir.display());
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => return (names, vec![cannot_list(error)]),
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                unreadable.push(cannot_list(error));
                continue;
            }
        };
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            unreadable.push(format!("{} has a name that is not UTF-8", path.display()));
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        let module_name = file_type(&path).and_then(|kind| match kind {
            Some(kind) if kind.is_dir() => Ok(file_type(&path.join("mod.rs"))?
                .is_some_and(|mod_kind| mod_kind.is_file())
                .then(|| file_name.to_owned())),
            Some(_) => Ok(file_name
                .strip_suffix(".rs")
                .filter(|stem| *stem != "main")
                .map(str::to_owned)),
            None => Ok(None),
        });
        match module_name {
            Ok(Some(name)) => {
                names.insert(name);
            }
            Ok(None) => {}
            Err(error) => unreadable.push(error),
        }
    }
    (names, unreadable)
}

/// Names of the files `main.rs` loads with `mod <name>;` (any visibility, `r#` names). It walks
/// the top-level items of the code without comments: an inline `mod name { ... }` is skipped
/// whole, so a `mod` nested in it (which loads a file from a subdirectory) does not count, and
/// neither does a declaration after `#[path = ...]`, which loads another file.
fn declared_modules(source: &str) -> BTreeSet<String> {
    // A byte order mark is not whitespace for `trim_start`; rustc and rustfmt accept it.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let code = code_without_comments_or_strings(source);
    let mut declared = BTreeSet::new();
    let mut follows_path_attribute = false;
    let mut rest = code.trim_start();
    while !rest.is_empty() {
        if let Some(after_hash) = rest.strip_prefix('#')
            && let attribute = after_hash
                .trim_start()
                .strip_prefix('!')
                .unwrap_or(after_hash)
            && let attribute = attribute.trim_start()
            && attribute.starts_with('[')
        {
            let (inner, after) = split_balanced(attribute);
            follows_path_attribute |= inner
                .trim_start_matches('[')
                .trim_start()
                .strip_prefix("path")
                .is_some_and(|after_path| after_path.trim_start().starts_with('='));
            rest = after.trim_start();
            continue;
        }
        let (item, after) = split_item(rest);
        if let Some(name) = declared_module_name(item)
            && !follows_path_attribute
        {
            declared.insert(name.to_owned());
        }
        follows_path_attribute = false;
        rest = after.trim_start();
    }
    declared
}

/// `name` if `item` is `[pub[(...)]] mod [r#]name;`.
fn declared_module_name(item: &str) -> Option<&str> {
    let declaration = match item.strip_prefix("pub") {
        Some(rest) if rest.trim_start().starts_with('(') => rest.split_once(')')?.1,
        Some(rest) => rest,
        None => item,
    };
    let name = declaration.trim_start().strip_prefix("mod")?;
    if !name.starts_with(char::is_whitespace) {
        return None;
    }
    let name = name.strip_suffix(';')?.trim();
    let name = name.strip_prefix("r#").unwrap_or(name);
    (!name.is_empty() && name.chars().all(|c| c == '_' || c.is_alphanumeric())).then_some(name)
}

/// Splits `code` after its first top-level item: the first `;` outside brackets, or the
/// bracket group that closes an item such as `mod name { ... }`.
fn split_item(code: &str) -> (&str, &str) {
    let mut depth = 0_usize;
    for (index, character) in code.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return code.split_at(index + 1);
                }
            }
            ';' if depth == 0 => return code.split_at(index + 1),
            _ => {}
        }
    }
    (code, "")
}

/// Splits `code`, which starts with an opening bracket, after the bracket that closes it.
fn split_balanced(code: &str) -> (&str, &str) {
    let mut depth = 0_usize;
    for (index, character) in code.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return code.split_at(index + 1);
                }
            }
            _ => {}
        }
    }
    (code, "")
}

/// `source` with each `//` comment and (nested) `/* */` comment replaced by whitespace, and each
/// string literal replaced by `""`, so that neither comment markers nor brackets inside a string
/// (such as a `#[path = "..."]` value) are read as code. Test entry points have no raw strings.
fn code_without_comments_or_strings(source: &str) -> String {
    let mut code = String::with_capacity(source.len());
    let mut block_depth = 0_usize;
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        match (character, characters.peek()) {
            ('/', Some('*')) => {
                characters.next();
                if block_depth == 0 {
                    // Like rustc, a comment separates tokens: `mod/**/x;` declares `x`.
                    code.push(' ');
                }
                block_depth += 1;
            }
            ('*', Some('/')) if block_depth > 0 => {
                characters.next();
                block_depth -= 1;
            }
            _ if block_depth > 0 => {}
            ('/', Some('/')) => {
                for skipped in characters.by_ref() {
                    if skipped == '\n' {
                        code.push('\n');
                        break;
                    }
                }
            }
            ('"', _) => {
                code.push_str("\"\"");
                let mut is_escaped = false;
                for skipped in characters.by_ref() {
                    match skipped {
                        '\\' if !is_escaped => is_escaped = true,
                        '"' if !is_escaped => break,
                        _ => is_escaped = false,
                    }
                }
            }
            _ => code.push(character),
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::{
        PackageTests, TestTarget, all_problems, declared_modules, layout_problems, verify,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    /// A package directory under the system temporary directory, removed on drop.
    struct PackageDir(PathBuf);

    impl PackageDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "emusync-xtask-layout-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("tests")).unwrap();
            Self(path)
        }

        fn write(&self, relative_path: &str, contents: &str) {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn target(&self, name: &str, relative_path: &str) -> TestTarget {
            TestTarget {
                name: name.to_owned(),
                source_path: self.0.join(relative_path),
            }
        }

        fn integration_target(&self) -> TestTarget {
            self.target("integration", "tests/integration/main.rs")
        }

        fn package(&self, test_targets: Vec<TestTarget>) -> PackageTests {
            PackageTests {
                name: "synthetic".to_owned(),
                dir: self.0.clone(),
                test_targets,
            }
        }

        fn problems(&self, test_targets: Vec<TestTarget>) -> Vec<String> {
            layout_problems(&self.package(test_targets))
        }
    }

    impl Drop for PackageDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mentions(problems: &[String], text: &str) -> bool {
        problems.iter().any(|problem| problem.contains(text))
    }

    #[test]
    fn declared_modules_accept_any_visibility_raw_names_and_trailing_comments() {
        let source = "//! doc\nmod common; // shared helpers\npub mod a_tests;\n\
                      pub(crate) mod b_tests;\nmod r#c_tests;\nmod inline {}\n";
        let declared = declared_modules(source);
        assert_eq!(
            declared.into_iter().collect::<Vec<_>>(),
            ["a_tests", "b_tests", "c_tests", "common"]
        );
    }

    #[test]
    fn declared_modules_follow_multiline_attributes_comments_and_strings() {
        let source = "#[path = \"other.rs\"]\n#[cfg(all(\n    unix,\n    not(windows),\n))]\n\
                      mod redirected_tests;\n/* Windows only */ mod prefixed_tests;\n\
                      #[path = \"a]//b.rs\"]\nmod tricky_tests;\nmod after_tricky_tests;\n\
                      mod inline {\n    fn helper() { let _ = [1, 2]; }\n    mod nested_tests;\n}\n\
                      mod after_inline_tests;\n# [path = \"x.rs\"] mod spaced_path_tests;\n\
                      # ! [allow(unused)]\nmod after_inner_attribute_tests;\n";
        let declared = declared_modules(source);
        assert_eq!(
            declared.into_iter().collect::<Vec<_>>(),
            [
                "after_inline_tests",
                "after_inner_attribute_tests",
                "after_tricky_tests",
                "prefixed_tests"
            ]
        );
    }

    #[test]
    fn declared_modules_ignore_declarations_inside_comments() {
        let source = "mod kept;\n// mod line_comment;\n/*\nmod block_comment;\n\
                      /* mod nested; */\nmod still_in_block;\n*/\nmod after_block;\n";
        let declared = declared_modules(source);
        assert_eq!(
            declared.into_iter().collect::<Vec<_>>(),
            ["after_block", "kept"]
        );
    }

    #[test]
    fn complete_layout_with_helpers_fixtures_and_editor_files_has_no_problems() {
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "mod common;\n\nmod a_tests;\n");
        package.write("tests/integration/common/mod.rs", "");
        package.write("tests/integration/a_tests.rs", "");
        package.write("tests/integration/.#a_tests.rs", "");
        package.write("tests/fixtures/comparison.tsv", "");
        package.write("tests/support/mod.rs", "");
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn undeclared_test_file_and_helper_directory_are_reported() {
        let package = PackageDir::new();
        package.write(
            "tests/integration/main.rs",
            "mod a_tests;\n/* mod forgotten_tests; */\n",
        );
        package.write("tests/integration/a_tests.rs", "");
        package.write("tests/integration/forgotten_tests.rs", "");
        package.write("tests/integration/common/mod.rs", "");
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
        assert!(mentions(&problems, "`mod common;`"), "{problems:?}");
    }

    #[test]
    fn any_test_target_besides_the_integration_binary_is_reported() {
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "");
        let loose = package.target("loose_tests", "tests/loose_tests.rs");
        let renamed = package.target("integration", "tests/other/main.rs");
        let problems = package.problems(vec![package.integration_target(), loose, renamed]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(mentions(&problems, "`loose_tests`"), "{problems:?}");
        assert!(mentions(&problems, "other"), "{problems:?}");
    }

    #[test]
    fn integration_directory_that_cargo_does_not_build_is_the_only_problem_reported() {
        // Nothing in the directory compiles, so the undeclared file is not reported as well.
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "");
        package.write("tests/integration/a_tests.rs", "");
        let problems = package.problems(Vec::new());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(mentions(&problems, "does not build"), "{problems:?}");
    }

    #[test]
    fn integration_directory_without_main_is_reported_as_missing() {
        let package = PackageDir::new();
        package.write("tests/integration/a_tests.rs", "");
        let problems = package.problems(Vec::new());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(mentions(&problems, "main.rs is missing"), "{problems:?}");
    }

    #[test]
    fn unreadable_package_does_not_hide_the_problems_of_the_others() {
        let forgetful = PackageDir::new();
        forgetful.write("tests/integration/main.rs", "");
        forgetful.write("tests/integration/forgotten_tests.rs", "");
        let unreadable = PackageDir::new();
        fs::create_dir_all(unreadable.0.join("tests/integration/main.rs")).unwrap();
        let problems = all_problems(&[
            forgetful.package(vec![forgetful.integration_target()]),
            unreadable.package(vec![unreadable.integration_target()]),
        ]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
        assert!(mentions(&problems, "cannot read"), "{problems:?}");
    }

    #[test]
    fn unreadable_main_that_cargo_does_not_build_reports_both_problems() {
        let package = PackageDir::new();
        fs::create_dir_all(package.0.join("tests/integration/main.rs")).unwrap();
        let problems = package.problems(Vec::new());
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(mentions(&problems, "cannot read"), "{problems:?}");
        assert!(mentions(&problems, "does not build"), "{problems:?}");
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_test_directory_is_reported_without_hiding_other_problems() {
        use std::os::unix::fs::PermissionsExt;
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "");
        package.write("tests/integration/new_tests/mod.rs", "");
        package.write("tests/integration/forgotten_tests.rs", "");
        let loose = package.target("loose_tests", "tests/loose_tests.rs");
        let locked = package.0.join("tests/integration/new_tests");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let permissions_enforced = fs::metadata(locked.join("mod.rs")).is_err();
        let problems = package.problems(vec![package.integration_target(), loose]);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if !permissions_enforced {
            return; // root reads anything: there is no error to report
        }
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(mentions(&problems, "cannot read"), "{problems:?}");
        assert!(mentions(&problems, "new_tests"), "{problems:?}");
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
        assert!(mentions(&problems, "`loose_tests`"), "{problems:?}");
    }

    #[test]
    fn package_without_integration_tests_has_no_problems() {
        let package = PackageDir::new();
        assert_eq!(package.problems(Vec::new()), Vec::<String>::new());
    }

    fn sorted(declared: std::collections::BTreeSet<String>) -> Vec<String> {
        declared.into_iter().collect()
    }

    #[test]
    fn main_saved_with_a_byte_order_mark_still_declares_its_first_module() {
        // Windows editors may save UTF-8 with a BOM; rustc and rustfmt accept it.
        let source = "\u{feff}mod first_tests;\nmod second_tests;\n";
        assert_eq!(
            sorted(declared_modules(source)),
            ["first_tests", "second_tests"]
        );
    }

    #[test]
    fn windows_line_endings_and_a_missing_final_newline_do_not_change_the_declarations() {
        let source = "//! Doc\r\nmod a_tests;\r\n#[path = \"x.rs\"]\r\nmod redirected_tests;\r\n\
                      // mod commented_tests;\r\n/* mod block_tests; */\r\nmod b_tests;";
        assert_eq!(sorted(declared_modules(source)), ["a_tests", "b_tests"]);
    }

    #[test]
    fn declarations_spread_over_lines_or_sharing_one_line_are_all_found() {
        let source = "pub\n(\n  crate\n)\nmod\n\tspread_tests\n;\n\
                      mod one_tests;mod two_tests ;pub(in crate)mod three_tests;\n\
                      pub(crate)\tmod\tr#tabbed_tests\t;\n";
        assert_eq!(
            sorted(declared_modules(source)),
            [
                "one_tests",
                "spread_tests",
                "tabbed_tests",
                "three_tests",
                "two_tests"
            ]
        );
    }

    #[test]
    fn declaration_with_a_block_comment_glued_to_the_keyword_is_found() {
        // rustc reads a comment as whitespace, so this declares `glued_tests`.
        let source = "mod/* reason */glued_tests;\npub/**/mod public_glued_tests;\n";
        assert_eq!(
            sorted(declared_modules(source)),
            ["glued_tests", "public_glued_tests"]
        );
    }

    #[test]
    fn stray_quotes_inside_comments_do_not_swallow_later_declarations() {
        let source = "// it's \"quoted\nmod after_line_quote_tests;\n\
                      /* \" unclosed */ mod after_block_quote_tests;\n\
                      /// Doc with a \" quote.\nmod after_doc_quote_tests;\n\
                      /** Block doc \" */\nmod after_block_doc_tests;\n";
        assert_eq!(
            sorted(declared_modules(source)),
            [
                "after_block_doc_tests",
                "after_block_quote_tests",
                "after_doc_quote_tests",
                "after_line_quote_tests"
            ]
        );
    }

    #[test]
    fn escaped_backslashes_and_quotes_end_strings_where_rust_ends_them() {
        let source = r#"#![doc = "mod inner_doc_tests; /* not a comment"]
#[doc = "C:\\tests\\"]
mod after_backslash_tests;
#[doc = "say \"mod hidden_tests;\" here"]
mod after_escaped_quote_tests;
#[path = "dir\\"]
mod redirected_tests;
#[doc = "quote \" then ]"]
mod after_bracket_in_string_tests;
mod last_tests;
"#;
        assert_eq!(
            sorted(declared_modules(source)),
            [
                "after_backslash_tests",
                "after_bracket_in_string_tests",
                "after_escaped_quote_tests",
                "last_tests"
            ]
        );
    }

    #[test]
    fn path_attribute_still_redirects_across_doc_comments_and_other_attributes() {
        let source = "#[path = \"shared.rs\"]\n/// Shared helpers.\n#[allow(dead_code)]\n\
                      // Reason.\npub(crate) mod redirected_tests;\nmod kept_tests;\n\
                      #[cfg(unix)]\n#[path = \"unix.rs\"]\nmod also_redirected_tests;\n\
                      #[pathological]\nmod not_a_path_attribute_tests;\n";
        assert_eq!(
            sorted(declared_modules(source)),
            ["kept_tests", "not_a_path_attribute_tests"]
        );
    }

    #[test]
    fn submodule_directories_data_and_editor_leftovers_are_not_modules() {
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "mod sync_tests;\n");
        package.write("tests/integration/sync_tests.rs", "mod helpers;\n");
        // 2018-style submodules of `sync_tests.rs`: the directory has no `mod.rs`.
        package.write("tests/integration/sync_tests/helpers.rs", "");
        package.write("tests/integration/data/save.bin", "");
        package.write("tests/integration/support/inner/mod.rs", "");
        package.write("tests/integration/notes.md", "");
        package.write("tests/integration/old_tests.rs.bak", "");
        package.write("tests/integration/scratch_tests.rs~", "");
        package.write("tests/integration/.hidden/mod.rs", "");
        package.write("tests/integration/.hidden_tests.rs", "");
        // A `mod.rs` that is a directory does not make its parent a module.
        fs::create_dir_all(package.0.join("tests/integration/fixtures/mod.rs")).unwrap();
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn unreadable_main_that_cargo_builds_reports_the_read_error_with_its_path() {
        // Without readable declarations the modules are not compared.
        let package = PackageDir::new();
        let main_path = package.0.join("tests").join("integration").join("main.rs");
        fs::create_dir_all(&main_path).unwrap();
        package.write("tests/integration/a_tests.rs", "");
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(mentions(&problems, "cannot read"), "{problems:?}");
        assert!(
            mentions(&problems, &main_path.display().to_string()),
            "{problems:?}"
        );
    }

    #[test]
    fn main_that_is_not_utf8_is_reported_with_its_path_alongside_other_targets() {
        let package = PackageDir::new();
        let main_path = package.0.join("tests").join("integration").join("main.rs");
        package.write("tests/integration/a_tests.rs", "");
        fs::write(&main_path, b"mod a_tests;\n\xff\xfe\n").unwrap();
        let loose = package.target("loose_tests", "tests/loose_tests.rs");
        let problems = package.problems(vec![package.integration_target(), loose]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(mentions(&problems, "`loose_tests`"), "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("cannot read")
                    && problem.contains(&main_path.display().to_string())),
            "{problems:?}"
        );
    }

    #[test]
    fn test_targets_outside_the_integration_binary_are_reported_even_without_a_tests_directory() {
        let renamed = PackageDir::new();
        renamed.write("tests/integration/main.rs", "");
        let alias = renamed.target("it", "tests/integration/main.rs");
        let renamed_problems = renamed.problems(vec![alias]);
        assert!(mentions(&renamed_problems, "`it`"), "{renamed_problems:?}");

        let elsewhere = PackageDir::new();
        fs::remove_dir(elsewhere.0.join("tests")).unwrap();
        let smoke = elsewhere.target("smoke", "checks/smoke.rs");
        let elsewhere_problems = elsewhere.problems(vec![smoke]);
        assert_eq!(elsewhere_problems.len(), 1, "{elsewhere_problems:?}");
        assert!(
            mentions(&elsewhere_problems, "`smoke`"),
            "{elsewhere_problems:?}"
        );
    }

    #[test]
    fn verify_fails_when_any_package_has_a_problem_and_passes_when_none_has() {
        let clean = PackageDir::new();
        clean.write("tests/integration/main.rs", "mod a_tests;\n");
        clean.write("tests/integration/a_tests.rs", "");
        let forgetful = PackageDir::new();
        forgetful.write("tests/integration/main.rs", "");
        forgetful.write("tests/integration/forgotten_tests.rs", "");
        let clean_package = || clean.package(vec![clean.integration_target()]);
        assert!(verify(&[clean_package()]).is_ok());
        let result = verify(&[
            clean_package(),
            forgetful.package(vec![forgetful.integration_target()]),
        ]);
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unlistable_integration_directory_reports_each_error_with_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let locked = PackageDir::new();
        locked.write("tests/integration/main.rs", "");
        locked.write("tests/integration/a_tests.rs", "");
        let forgetful = PackageDir::new();
        forgetful.write("tests/integration/main.rs", "");
        forgetful.write("tests/integration/forgotten_tests.rs", "");
        let integration_dir = locked.0.join("tests/integration");
        let main_path = integration_dir.join("main.rs");
        fs::set_permissions(&integration_dir, fs::Permissions::from_mode(0o000)).unwrap();
        let permissions_enforced = fs::read_dir(&integration_dir).is_err();
        let problems = all_problems(&[
            locked.package(vec![
                locked.integration_target(),
                locked.target("loose_tests", "tests/loose_tests.rs"),
            ]),
            forgetful.package(vec![forgetful.integration_target()]),
        ]);
        fs::set_permissions(&integration_dir, fs::Permissions::from_mode(0o755)).unwrap();
        if !permissions_enforced {
            return; // root reads anything: there is no error to report
        }
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("cannot read")
                    && problem.contains(&main_path.display().to_string())),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("cannot list")
                    && problem.contains(&integration_dir.display().to_string())),
            "{problems:?}"
        );
        assert!(mentions(&problems, "`loose_tests`"), "{problems:?}");
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_tests_directory_is_reported_with_its_path_beside_the_loose_target() {
        use std::os::unix::fs::PermissionsExt;
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "");
        let tests_dir = package.0.join("tests");
        fs::set_permissions(&tests_dir, fs::Permissions::from_mode(0o000)).unwrap();
        let permissions_enforced = fs::metadata(tests_dir.join("integration")).is_err();
        let problems = package.problems(vec![
            package.integration_target(),
            package.target("loose_tests", "tests/loose_tests.rs"),
        ]);
        fs::set_permissions(&tests_dir, fs::Permissions::from_mode(0o755)).unwrap();
        if !permissions_enforced {
            return; // root reads anything: there is no error to report
        }
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(mentions(&problems, "`loose_tests`"), "{problems:?}");
        let integration_dir = tests_dir.join("integration").display().to_string();
        assert!(
            problems.iter().any(
                |problem| problem.contains("cannot read") && problem.contains(&integration_dir)
            ),
            "{problems:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_in_the_test_directory_is_reported_without_hiding_other_files() {
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "mod linked_tests;\n");
        package.write("shared/linked.rs", "");
        package.write("tests/integration/forgotten_tests.rs", "");
        let integration_dir = package.0.join("tests/integration");
        let loop_path = integration_dir.join("loop_tests.rs");
        std::os::unix::fs::symlink("loop_tests.rs", &loop_path).unwrap();
        std::os::unix::fs::symlink(
            package.0.join("shared/linked.rs"),
            integration_dir.join("linked_tests.rs"),
        )
        .unwrap();
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("cannot read")
                    && problem.contains(&loop_path.display().to_string())),
            "{problems:?}"
        );
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn file_name_that_is_not_utf8_is_reported_without_hiding_other_files() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let package = PackageDir::new();
        package.write("tests/integration/main.rs", "");
        package.write("tests/integration/forgotten_tests.rs", "");
        let integration_dir = package.0.join("tests/integration");
        fs::write(
            integration_dir.join(OsStr::from_bytes(b"bad\xffname_tests.rs")),
            "",
        )
        .unwrap();
        let problems = package.problems(vec![package.integration_target()]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(mentions(&problems, "not UTF-8"), "{problems:?}");
        assert!(
            mentions(&problems, "`mod forgotten_tests;`"),
            "{problems:?}"
        );
    }
}
