use std::collections::HashMap;
use std::fs::{self, File, read_link};
use std::io::{BufRead, BufReader};

fn read_memtotal_kb() -> Option<u64> {
    let file = File::open("/proc/meminfo").ok()?;
    for line in BufReader::new(file).lines().flatten() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            return rest.split_whitespace().next()?.parse::<u64>().ok();
        }
    }
    None
}

/// Extracts the command name (argv[0] basename) from /proc/[pid]/cmdline
fn read_cmdname(pid: &str) -> Option<String> {
    let data = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let mut parts = data.split(|b| *b == 0u8);
    let argv0 = parts.next()?.split(|b| *b == b' ').next()?; // remove trailing args if embedded

    if argv0.is_empty() {
        return None;
    }

    let cmd = String::from_utf8_lossy(argv0).to_string();
    let path = std::path::Path::new(&cmd);
    path.file_name().map(|s| s.to_string_lossy().to_string())
}

fn read_status_vmrss_kb(pid: &str) -> Option<u64> {
    let file = File::open(format!("/proc/{pid}/status")).ok()?;
    for line in BufReader::new(file).lines().flatten() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse::<u64>().ok();
        }
    }
    Some(0)
}

fn read_cmdline(pid: &str) -> Option<Vec<String>> {
    let data = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if data.is_empty() {
        return Some(vec![]);
    }
    let parts = data
        .split(|b| *b == 0u8)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect::<Vec<_>>();
    Some(parts)
}

fn exe_basename(pid: &str) -> Option<String> {
    let p = read_link(format!("/proc/{pid}/exe")).ok()?;
    Some(p.file_name()?.to_string_lossy().to_string())
}

#[derive(Clone, Copy)]
enum JavaStrategy {
    Auto,
    Jar,
    Main,
}

fn parse_java_strategy(arg: Option<String>) -> JavaStrategy {
    match arg.as_deref() {
        Some("--java-by=jar") => JavaStrategy::Jar,
        Some("--java-by=main") => JavaStrategy::Main,
        _ => JavaStrategy::Auto,
    }
}

fn find_jar_name(cmdline: &[String]) -> Option<String> {
    // Looks for "-jar <file>", returns the JAR's basename
    let mut i = 1; // skip argv[0] ("java")
    while i < cmdline.len() {
        let tok = &cmdline[i];
        if tok == "-jar" {
            if i + 1 < cmdline.len() {
                let jar = std::path::Path::new(&cmdline[i + 1]);
                return jar.file_name().map(|f| f.to_string_lossy().to_string());
            } else {
                return None;
            }
        }
        if tok.starts_with('-') {
            // skip JVM options; handle options with a separate argument
            if tok == "-cp" || tok == "-classpath" || tok == "--class-path" {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        break;
    }
    None
}

fn find_main_class(cmdline: &[String]) -> Option<String> {
    // Skips JVM options to the first non-option token (the main class)
    let mut i = 1; // skip "java"
    while i < cmdline.len() && cmdline[i].starts_with('-') {
        if cmdline[i] == "-cp" || cmdline[i] == "-classpath" || cmdline[i] == "--class-path" {
            i += 2;
        } else {
            i += 1;
        }
    }
    cmdline.get(i).cloned()
}

/// Try to produce a nicer name for a Java process:
/// - If "-jar X" is present -> basename(X)
/// - Else first non-option token after JVM flags -> main class
fn java_display_name(cmdline: &[String], strat: JavaStrategy) -> Option<String> {
    match strat {
        JavaStrategy::Jar => find_jar_name(cmdline),
        JavaStrategy::Main => find_main_class(cmdline),
        JavaStrategy::Auto => find_jar_name(cmdline).or_else(|| find_main_class(cmdline)),
    }
}

fn is_numeric_dir(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_digit())
}

struct MapEntry {
    num: u32,
    memory: u64,
}

fn main() {
    // Args: [limit] [--java-by=auto|jar|main]
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let java_arg = args.iter().find(|a| a.starts_with("--java-by=")).cloned();
    args.retain(|a| !a.starts_with("--java-by="));
    let limit: usize = args.get(0).and_then(|s| s.parse().ok()).unwrap_or(20);
    let jstrategy = parse_java_strategy(java_arg);

    let total_kb = match read_memtotal_kb() {
        Some(v) if v > 0 => v,
        _ => {
            eprintln!("Could not read MemTotal from /proc/meminfo");
            std::process::exit(1);
        }
    };

    let mut by_key: HashMap<String, MapEntry> = HashMap::new();

    let proc = match fs::read_dir("/proc") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read /proc: {e}");
            std::process::exit(1);
        }
    };

    for entry in proc.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !is_numeric_dir(&name) {
            continue;
        }

        // Processes vanish; ignore errors quietly.
        let rss_kb = match read_status_vmrss_kb(&name) {
            Some(v) => v,
            None => continue,
        };
        if rss_kb == 0 {
            continue;
        }

        let comm = match read_cmdname(&name) {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };

        let key = if comm == "java" || comm == "javaw" {
            let cmdline = read_cmdline(&name).unwrap_or_default();
            if let Some(app) = java_display_name(&cmdline, jstrategy) {
                let app = app.rsplit('.').next().unwrap_or(&app).to_string();
                format!("java: {}", app)
            } else {
                let exe = exe_basename(&name).unwrap_or_else(|| "java".to_string());
                format!("java ({exe})")
            }
        } else {
            comm
        };

        by_key.entry(key)
            .and_modify(|e| {
                e.num += 1;
                e.memory += rss_kb;
            })
            .or_insert(MapEntry {num: 1, memory: rss_kb});
    }

    let mut rows: Vec<(String, MapEntry)> = by_key.into_iter().collect();
    rows.sort_by(|a, b| b.1.memory.cmp(&a.1.memory));

    println!("{:<35} {:>4} {:>12} {:>8} {:>8}", "Application", "Num", "Memory(MB)", "%", "Cum.%");
    let mut cum = 0.0_f64;
    for (key, entry) in rows.into_iter().take(limit) {
        let mb = (entry.memory as f64) / 1024.0;
        let pct = (entry.memory as f64) * 100.0 / (total_kb as f64);
        cum += pct;
        println!("{:<35} {:>4} {:>12.2} {:>7.2}% {:>7.2}%", key, entry.num, mb, pct, cum);
    }
}

