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
//! Each case is a test of its own, named by its path, so
//! `cargo test --test kramdown_suite block/04_header` runs only those.
//!
//! Cases known to fail are listed in known-failures.txt, a case on each line
//! with an optional `# reason`. A listed case's test, marked
//! `[known failure]`, passes while the case fails, and fails once it passes.
//! A listed case that isn't one fails a test of its own. With
//! `UPDATE_KRAMDOWN_SUITE=1`, it rewrites the list to the cases failing now
//! instead, keeping the reasons of those already listed.

mod diff;
mod normalise;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use libtest_mimic::{Arguments, Trial};
use pulldown_kram::{Document, Extensions, html};

use diff::diff;
use normalise::Normaliser;

/// Where the suite's files are.
const SUITE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/kramdown_suite");

/// The environment variable that rewrites the list of known failures.
const UPDATE: &str = "UPDATE_KRAMDOWN_SUITE";

/// The normaliser every case shares.
static NORMALISER: LazyLock<Normaliser> = LazyLock::new(Normaliser::load);

thread_local! {
    /// Whether this thread is checking a case, whose panics are reported as
    /// its failure instead.
    static CHECKING: Cell<bool> = const { Cell::new(false) };
}

fn main() {
    let args = Arguments::from_args();
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !CHECKING.get() {
            hook(info);
        }
    }));
    let known = KnownFailures::load();
    let cases = Case::all();

    if std::env::var_os(UPDATE).is_some() {
        let failed: Vec<_> = cases.iter().filter_map(Case::failure).collect();
        known.rewrite(&failed);
        println!("rewrote known-failures.txt with {} cases", failed.len());
        return;
    }
    let mut trials = normalise::tests::trials();
    trials.extend(known.missing(&cases));
    trials.extend(cases.into_iter().map(|case| {
        let listed = known.contains(&case.name);
        case.trial(listed)
    }));
    libtest_mimic::run(&args, trials).exit();
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

    /// The case's test: that it passes, or, `listed` as known to fail, that
    /// it still fails.
    fn trial(self, listed: bool) -> Trial {
        let name = self.name.clone();
        if listed {
            return Trial::test(name, move || match self.check() {
                Ok(()) => Err(format!(
                    "listed in known-failures.txt, but passes; \
                     rerun with {UPDATE}=1 to accept it"
                )
                .into()),
                Err(_) => Ok(()),
            })
            .with_kind("known failure");
        }
        Trial::test(name, move || match self.failure() {
            Some(failure) => {
                Err(format!("{failure}\nrerun with {UPDATE}=1 to accept it").into())
            }
            None => Ok(()),
        })
    }

    /// How the case fails, if it does.
    fn failure(&self) -> Option<Failure> {
        let difference = self.check().err()?;
        Some(Failure {
            name: self.name.clone(),
            defaulted: self.defaulted.clone(),
            difference,
        })
    }

    /// Checks that pulldown-kram's HTML for the case is kramdown's, once
    /// both are normalised, returning how they differ, or how it panicked,
    /// if not.
    fn check(&self) -> Result<(), String> {
        let normaliser = &*NORMALISER;
        let expected = normaliser.normalise(&self.html);
        CHECKING.set(true);
        let actual = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut html = String::new();
            let document = Document::new(&self.text, self.extensions);
            html::push_html(&mut html, document.events());
            normaliser.normalise(&html)
        }));
        CHECKING.set(false);
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

    /// A failing test for each listed case that isn't one.
    fn missing(&self, cases: &[Case]) -> Vec<Trial> {
        self.0
            .keys()
            .filter(|name| !cases.iter().any(|case| &case.name == *name))
            .map(|name| {
                Trial::test(name.clone(), || {
                    Err("listed in known-failures.txt, but isn't a case".into())
                })
                .with_kind("known failure")
            })
            .collect()
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
