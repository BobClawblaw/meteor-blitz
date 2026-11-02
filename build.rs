//! Windows: embeds the app icon (assets/icon/icon.ico, tools/make-icon.py)
//! and version info in the executable. Every target gets `MB_BUILD_DATE`,
//! the date shown beside the version on the main menu.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
    println!("cargo:rustc-env=MB_BUILD_DATE={}", build_date());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set("ProductName", "Meteor Blitz");
        res.set("FileDescription", "Meteor Blitz");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}

/// The date of the commit being built (YYYY-MM-DD), so a release shows the
/// day it was cut; without git, today's date (UTC).
fn build_date() -> String {
    let git = std::process::Command::new("git").args(["log", "-1", "--format=%cs"]).output();
    if let Ok(out) = git {
        let date = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && date.len() == 10 {
            return date;
        }
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
