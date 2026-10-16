//! Embed the built source revision so `--version` and manifests can name the
//! exact build even when two binaries share a crate version.
//!
//! The revision is the tool repository's `git rev-parse --short=8 HEAD` at
//! build time. It is exposed to the crate as the `CGFX_TOOLS_REVISION`
//! environment variable and folded into the generator identity by `cgfx-core`.
//! Same checked-out revision rebuilds the same identity; a different revision
//! (or an unavailable git tree) yields a different token.

use std::process::Command;

fn main() {
    // No `rerun-if-changed` directive: the embedded revision tracks `git HEAD`,
    // which is not a watched file, so the build script must run on every build.
    // Otherwise the token is cached from the first compile in a target dir and
    // goes stale when HEAD moves (the bug that pinned `g543e2c22` at 7b60cca).
    let revision = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=CGFX_TOOLS_REVISION={revision}");
}
