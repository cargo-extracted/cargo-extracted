//! Differential cases: Cargo's answer versus the extracted resolver's.

use conformance::{Case, Dep, Options, Pkg, run, run_with};

fn manifest(name: &str, edition: &str, body: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n\n{body}")
}

#[test]
fn picks_highest_compatible_version() {
    let case = Case::new("highest")
        .pkg(Pkg::new("log", "0.4.1"))
        .pkg(Pkg::new("log", "0.4.20"))
        .pkg(Pkg::new("log", "0.5.0"))
        .package(&manifest("app", "2021", "[dependencies]\nlog = \"0.4\"\n"));
    run(&case).unwrap().assert_ok();
}

#[test]
fn transitive_dependencies_and_duplicates() {
    let case = Case::new("transitive")
        .pkg(Pkg::new("base", "1.0.0"))
        .pkg(Pkg::new("base", "2.0.0"))
        .pkg(Pkg::new("mid", "0.3.0").dep(Dep::new("base", "^1")))
        .pkg(
            Pkg::new("top", "0.1.0")
                .dep(Dep::new("mid", "^0.3"))
                .dep(Dep::new("base", "^2")),
        )
        .package(&manifest("app", "2021", "[dependencies]\ntop = \"0.1\"\n"));
    run(&case).unwrap().assert_ok();
}

#[test]
fn semver_compatible_versions_unify() {
    let case = Case::new("unify")
        .pkg(Pkg::new("shared", "1.2.0"))
        .pkg(Pkg::new("shared", "1.5.0"))
        .pkg(Pkg::new("shared", "1.9.0"))
        .pkg(Pkg::new("a", "1.0.0").dep(Dep::new("shared", "^1.2")))
        .pkg(Pkg::new("b", "1.0.0").dep(Dep::new("shared", "<1.6")))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\na = \"1\"\nb = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn yanked_versions_are_skipped() {
    let case = Case::new("yanked")
        .pkg(Pkg::new("lib", "1.0.0"))
        .pkg(Pkg::new("lib", "1.0.1").yanked())
        .pkg(Pkg::new("lib", "1.1.0").yanked())
        .package(&manifest("app", "2021", "[dependencies]\nlib = \"1\"\n"));
    run(&case).unwrap().assert_ok();
}

#[test]
fn optional_dependencies_and_features() {
    let case = Case::new("features")
        .pkg(Pkg::new("serde", "1.0.0"))
        .pkg(
            Pkg::new("serde", "1.0.9")
                .dep(Dep::new("serde_derive", "=1.0.9").optional())
                .feature("default", &["std"])
                .feature("std", &[])
                .feature("derive", &["serde_derive"]),
        )
        .pkg(Pkg::new("serde_derive", "1.0.9"))
        .pkg(
            Pkg::new("fancy", "0.2.0")
                .dep(Dep::new("serde", "^1").optional().no_default_features())
                .dep(Dep::new("extra", "^0.1").optional())
                .feature2("default", &["dep:extra"])
                .feature2("with-serde", &["dep:serde", "serde/derive"]),
        )
        .pkg(Pkg::new("extra", "0.1.0"))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nfancy = { version = \"0.2\", default-features = false, features = [\"with-serde\"] }\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn weak_dependency_features() {
    let case = Case::new("weak-features")
        .pkg(Pkg::new("core", "1.0.0").feature("alloc", &[]))
        .pkg(
            Pkg::new("lib", "1.0.0")
                .dep(Dep::new("core", "^1").optional())
                .feature2("alloc", &["core?/alloc"])
                .feature2("core", &["dep:core"]),
        )
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nlib = { version = \"1\", features = [\"alloc\"] }\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn dev_and_build_dependencies() {
    let case = Case::new("dev-build")
        .pkg(Pkg::new("cc", "1.0.0"))
        .pkg(Pkg::new("cc", "1.2.0"))
        .pkg(Pkg::new("testlib", "0.9.0"))
        .pkg(Pkg::new("sys", "0.1.0").dep(Dep::new("cc", "^1.0").build()).dep(Dep::new("testlib", "^0.9").dev()))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nsys = \"0.1\"\n\n[dev-dependencies]\ntestlib = \"0.9\"\n\n[build-dependencies]\ncc = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn platform_specific_dependencies_are_all_locked() {
    let case = Case::new("platform")
        .pkg(Pkg::new("winapi", "0.3.0"))
        .pkg(Pkg::new("libc", "0.2.0"))
        .pkg(
            Pkg::new("portable", "1.0.0")
                .dep(Dep::new("winapi", "^0.3").target("cfg(windows)"))
                .dep(Dep::new("libc", "^0.2").target("cfg(unix)")),
        )
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nportable = \"1\"\n\n[target.'cfg(target_os = \"macos\")'.dependencies]\nlibc = \"0.2\"\n\n[target.x86_64-unknown-linux-gnu.dependencies]\nwinapi = \"0.3\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn renamed_dependencies() {
    let case = Case::new("rename")
        .pkg(Pkg::new("real-crate", "2.0.0"))
        .pkg(Pkg::new("real-crate", "1.0.0"))
        .pkg(Pkg::new("user", "0.1.0").dep(Dep::new("alias", "^1").package("real-crate")))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nuser = \"0.1\"\nnew = { package = \"real-crate\", version = \"2\" }\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn workspace_with_path_dependencies() {
    let case = Case::new("workspace")
        .pkg(Pkg::new("log", "0.4.0"))
        .pkg(Pkg::new("log", "0.4.8"))
        .virtual_root("[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n")
        .member(
            "crates/core",
            &manifest("core", "2021", "[dependencies]\nlog = \"0.4\"\n"),
        )
        .member(
            "crates/cli",
            &manifest(
                "cli",
                "2021",
                "[dependencies]\ncore = { path = \"../core\" }\nlog = \"0.4.5\"\n\n[dev-dependencies]\ncore = { path = \"../core\" }\n",
            ),
        );
    run(&case).unwrap().assert_ok();
}

#[test]
fn msrv_aware_resolution() {
    let case = Case::new("msrv")
        .pkg(Pkg::new("modern", "1.0.0").rust_version("1.60"))
        .pkg(Pkg::new("modern", "1.1.0").rust_version("1.80"))
        .pkg(Pkg::new("modern", "1.2.0").rust_version("1.86"))
        .pkg(Pkg::new("modern", "1.3.0").rust_version("1.99"))
        .package(&manifest(
            "app",
            "2024",
            "rust-version = \"1.86\"\n\n[dependencies]\nmodern = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn msrv_aware_resolution_with_explicit_resolver_three() {
    let case = Case::new("msrv-resolver3")
        .pkg(Pkg::new("modern", "1.0.0").rust_version("1.60"))
        .pkg(Pkg::new("modern", "1.1.0").rust_version("1.70"))
        .pkg(Pkg::new("modern", "1.2.0").rust_version("1.85"))
        .pkg(Pkg::new("modern", "1.3.0"))
        .package(&manifest(
            "app",
            "2021",
            "rust-version = \"1.70\"\nresolver = \"3\"\n\n[dependencies]\nmodern = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn msrv_lowest_member_wins_in_workspace() {
    let case = Case::new("msrv-workspace")
        .pkg(Pkg::new("modern", "1.0.0").rust_version("1.60"))
        .pkg(Pkg::new("modern", "1.1.0").rust_version("1.75"))
        .pkg(Pkg::new("modern", "1.2.0").rust_version("1.90"))
        .virtual_root("[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"3\"\n")
        .member(
            "a",
            &manifest(
                "a",
                "2021",
                "rust-version = \"1.70\"\n\n[dependencies]\nmodern = \"1\"\n",
            ),
        )
        .member(
            "b",
            &manifest(
                "b",
                "2021",
                "rust-version = \"1.95\"\n\n[dependencies]\nmodern = \"1\"\n",
            ),
        );
    run(&case).unwrap().assert_ok();
}

#[test]
fn msrv_fallback_to_rustc_without_rust_version() {
    let case = Case::new("msrv-rustc")
        .pkg(Pkg::new("modern", "1.0.0").rust_version("1.60"))
        .pkg(Pkg::new("modern", "1.1.0").rust_version("1.999"))
        .package(&manifest("app", "2024", "[dependencies]\nmodern = \"1\"\n"));
    run(&case).unwrap().assert_ok();
}

#[test]
fn resolver_two_ignores_msrv() {
    let case = Case::new("msrv-ignored")
        .pkg(Pkg::new("modern", "1.0.0").rust_version("1.60"))
        .pkg(Pkg::new("modern", "1.1.0").rust_version("1.999"))
        .package(&manifest(
            "app",
            "2021",
            "rust-version = \"1.70\"\n\n[dependencies]\nmodern = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn unsupported_index_entries_are_ignored() {
    let case = Case::new("index-v3")
        .pkg(Pkg::new("lib", "1.0.0"))
        .pkg(Pkg::new("lib", "1.5.0").schema_version(3))
        .package(&manifest("app", "2021", "[dependencies]\nlib = \"1\"\n"));
    run(&case).unwrap().assert_ok();
}

#[test]
fn prerelease_requirements() {
    let case = Case::new("prerelease")
        .pkg(Pkg::new("beta", "2.0.0-alpha.1"))
        .pkg(Pkg::new("beta", "2.0.0-beta.3"))
        .pkg(Pkg::new("beta", "1.9.0"))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\nbeta = \"2.0.0-alpha\"\nstable = { package = \"beta\", version = \"1\" }\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn links_conflicts_fail_on_both_sides() {
    let case = Case::new("links")
        .pkg(Pkg::new("z-sys", "1.0.0").links("z"))
        .pkg(Pkg::new("z-sys", "2.0.0").links("z"))
        .pkg(Pkg::new("a", "1.0.0").dep(Dep::new("z-sys", "^1")))
        .pkg(Pkg::new("b", "1.0.0").dep(Dep::new("z-sys", "^2")))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\na = \"1\"\nb = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn links_backtracks_to_a_compatible_version() {
    let case = Case::new("links-backtrack")
        .pkg(Pkg::new("z-sys", "1.0.0").links("z"))
        .pkg(Pkg::new("z-sys", "2.0.0").links("z"))
        .pkg(Pkg::new("a", "1.0.0").dep(Dep::new("z-sys", "^1")))
        .pkg(Pkg::new("b", "1.0.0").dep(Dep::new("z-sys", "^1")))
        .pkg(Pkg::new("b", "1.1.0").dep(Dep::new("z-sys", "^2")))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\na = \"1\"\nb = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn missing_dependency_fails_on_both_sides() {
    let case = Case::new("missing")
        .pkg(Pkg::new("present", "1.0.0").dep(Dep::new("absent", "^1")))
        .package(&manifest(
            "app",
            "2021",
            "[dependencies]\npresent = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

#[test]
fn lockfile_version_follows_rust_version() {
    let case = Case::new("lock-v3")
        .pkg(Pkg::new("lib", "1.0.0"))
        .package(&manifest(
            "app",
            "2021",
            "rust-version = \"1.60\"\n\n[dependencies]\nlib = \"1\"\n",
        ));
    run(&case).unwrap().assert_ok();
}

/// The comparison itself must notice a difference, or every test above is vacuous.
#[test]
fn comparison_detects_a_different_lockfile() {
    use conformance::{Outcome, compare::compare};
    let a = Outcome::from_lockfile("version = 4\n[[package]]\nname = \"x\"\nversion = \"1.0.0\"\n")
        .unwrap();
    let b = Outcome::from_lockfile("version = 4\n[[package]]\nname = \"x\"\nversion = \"1.0.1\"\n")
        .unwrap();
    assert!(!compare("self", &a, &b).ok);
    let c = Outcome::Failed("boom".into());
    assert!(!compare("self", &a, &c).ok);
    assert!(compare("self", &c, &Outcome::Failed("other".into())).ok);
}

/// End-to-end proof that a wrong answer from the subject is reported.
#[test]
fn sabotaged_subject_is_detected() {
    let case = Case::new("sabotage")
        .pkg(Pkg::new("log", "0.4.1"))
        .pkg(Pkg::new("log", "0.4.20"))
        .package(&manifest("app", "2021", "[dependencies]\nlog = \"0.4\"\n"));
    let report = run_with(
        &case,
        Options {
            sabotage_minimal_versions: true,
        },
    )
    .unwrap();
    assert!(!report.ok, "sabotage went unnoticed:\n{}", report.text);
    assert!(
        report.text.contains("only cargo has") && report.text.contains("0.4.20"),
        "{}",
        report.text
    );
}
