//! kramdown's own test cases, read with pulldown-kram and checked against
//! kramdown's HTML for them, as pulldown-cmark checks itself against the
//! CommonMark spec.
//!
//! The cases are in testcases/ here, copied from kramdown 2.5.2. Their HTML
//! is what kramdown makes of them with GFM input, as Jekyll runs it, with
//! kramdown's default in place of each option pulldown-kram doesn't
//! support; generate.rb makes it, and says how. It also writes cases.txt,
//! which says for each case which extensions its options leave off. Each
//! case is read with every other extension, and its events are written out
//! with `pulldown_kram::html`, which writes them the way kramdown writes the
//! same elements. Both sides are then normalised, to erase how the HTML is
//! formatted but not what it says, and compared.
//!
//! Cases known to fail are listed in known-failures.txt, a case on each line
//! with an optional `# reason`. The test fails when a case fails that isn't
//! listed, printing how it differs, or when a listed case passes. With
//! `UPDATE_KRAMDOWN_SUITE=1`, it rewrites the list to the cases failing now
//! instead, keeping the reasons of those already listed.

mod diff;
mod normalise;

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use pulldown_kram::{Document, Extensions, html};

use diff::diff;
use normalise::Normaliser;

/// Where the suite's files are.
const SUITE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/kramdown_suite");

/// The environment variable that rewrites the list of known failures.
const UPDATE: &str = "UPDATE_KRAMDOWN_SUITE";

#[test]
fn kramdown_suite() {
    let normaliser = Normaliser::load();
    let known = KnownFailures::load();
    let (passed, failed) = run(&normaliser);
    let known_failing = failed.iter().filter(|f| known.contains(&f.name)).count();
    println!(
        "kramdown suite: {} run, {} passing, {} failing ({known_failing} known)",
        passed.len() + failed.len(),
        passed.len(),
        failed.len(),
    );

    if std::env::var_os(UPDATE).is_some() {
        known.rewrite(&failed);
        println!("rewrote known-failures.txt with {} cases", failed.len());
        return;
    }
    let problems = known.problems(&passed, &failed);
    assert!(
        problems.is_empty(),
        "the kramdown suite differs from known-failures.txt; \
         rerun with {UPDATE}=1 to accept it\n{problems}"
    );
}

/// Checks every case, returning the names of those that pass, and the
/// failures.
fn run(normaliser: &Normaliser) -> (Vec<String>, Vec<Failure>) {
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    // Panics are failures of their case, reported with the rest.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    for case in Case::all() {
        match case.check(normaliser) {
            Ok(()) => passed.push(case.name),
            Err(difference) => failed.push(Failure {
                name: case.name,
                defaulted: case.defaulted,
                difference,
            }),
        }
    }
    panic::set_hook(hook);
    (passed, failed)
}

/// One of kramdown's test cases.
struct Case {
    /// Its path under testcases, without the extension, like
    /// `block/04_header/atx_header`.
    name: String,
    /// The Markdown.
    text: String,
    /// kramdown's HTML for it.
    html: String,
    /// The extensions to read it with: every one its options don't leave
    /// off.
    extensions: Extensions,
    /// The options it gives that pulldown-kram doesn't support, which
    /// kramdown converted it with its defaults for instead.
    defaulted: Vec<String>,
}

impl Case {
    /// Every case, in the order of cases.txt.
    fn all() -> Vec<Self> {
        let root = Path::new(SUITE).join("testcases");
        let manifest = fs::read_to_string(Path::new(SUITE).join("cases.txt")).unwrap();
        let list = |column: &str| -> Vec<String> {
            match column {
                "-" => Vec::new(),
                column => column.split(',').map(str::to_owned).collect(),
            }
        };
        manifest
            .lines()
            .map(|line| {
                let [name, without, defaulted] = line.split('\t').collect::<Vec<_>>()[..] else {
                    panic!("cases.txt: expected three columns in {line:?}");
                };
                let mut extensions = Extensions::all();
                for flag in list(without) {
                    extensions -= Extensions::from_name(&flag)
                        .unwrap_or_else(|| panic!("cases.txt: no extension {flag}"));
                }
                let path = root.join(name);
                Self {
                    name: name.to_owned(),
                    text: fs::read_to_string(path.with_extension("text")).unwrap(),
                    html: fs::read_to_string(path.with_extension("html")).unwrap(),
                    extensions,
                    defaulted: list(defaulted),
                }
            })
            .collect()
    }

    /// Checks that pulldown-kram's HTML for the case is kramdown's, once
    /// both are normalised, returning how they differ, or how it panicked,
    /// if not.
    fn check(&self, normaliser: &Normaliser) -> Result<(), String> {
        let expected = normaliser.normalise(&self.html);
        let actual = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut html = String::new();
            let document = Document::new(&self.text, self.extensions);
            html::push_html(&mut html, document.events());
            normaliser.normalise(&html)
        }));
        match actual {
            Ok(actual) if actual == expected => Ok(()),
            Ok(actual) => Err(diff(&expected, &actual)),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("");
                Err(format!("panicked: {message}\n"))
            }
        }
    }
}

/// A case that failed.
struct Failure {
    name: String,
    /// The options kramdown converted it with its defaults for.
    defaulted: Vec<String>,
    /// How pulldown-kram's HTML differs from kramdown's, or how it panicked.
    difference: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} fails", self.name)?;
        if !self.defaulted.is_empty() {
            let options = self.defaulted.join(", ");
            write!(f, " with kramdown's default for {options}, as unsupported")?;
        }
        write!(f, " (- kramdown, + pulldown-kram):\n{}", self.difference)
    }
}

/// The cases known to fail, from known-failures.txt, each with the reason
/// given for it, if any.
struct KnownFailures(BTreeMap<String, Option<String>>);

impl KnownFailures {
    fn path() -> PathBuf {
        Path::new(SUITE).join("known-failures.txt")
    }

    fn load() -> Self {
        let Ok(list) = fs::read_to_string(Self::path()) else {
            return Self(BTreeMap::new());
        };
        let cases = list.lines().filter_map(|line| {
            let (name, reason) = match line.split_once('#') {
                Some((name, reason)) => (name.trim(), Some(reason.trim())),
                None => (line.trim(), None),
            };
            let reason = reason.filter(|reason| !reason.is_empty());
            (!name.is_empty()).then(|| (name.to_owned(), reason.map(str::to_owned)))
        });
        Self(cases.collect())
    }

    fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Where the list is wrong: the cases failing that it doesn't list, with
    /// how they differ, and the cases it lists that pass or aren't run.
    fn problems(&self, passed: &[String], failed: &[Failure]) -> String {
        let mut problems = String::new();
        for failure in failed.iter().filter(|f| !self.contains(&f.name)) {
            writeln!(problems, "\n{failure}").unwrap();
        }
        for name in self.0.keys() {
            if passed.contains(name) {
                writeln!(problems, "\n{name} is listed as failing, but passes").unwrap();
            } else if !failed.iter().any(|failure| &failure.name == name) {
                writeln!(problems, "\n{name} is listed as failing, but isn't run").unwrap();
            }
        }
        problems
    }

    /// Rewrites the list to the cases in `failed`, keeping the reasons
    /// already given for them.
    fn rewrite(&self, failed: &[Failure]) {
        let mut list = String::from(
            "# kramdown's test cases pulldown-kram reads differently, checked by\n\
             # tests/kramdown_suite/main.rs. A case on each line, with an optional\n\
             # reason. UPDATE_KRAMDOWN_SUITE=1 cargo test --test kramdown_suite\n\
             # rewrites it.\n",
        );
        for Failure { name, .. } in failed {
            match self.0.get(name).cloned().flatten() {
                Some(reason) => writeln!(list, "{name} # {reason}").unwrap(),
                None => writeln!(list, "{name}").unwrap(),
            }
        }
        fs::write(Self::path(), list).unwrap();
    }
}
