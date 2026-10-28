//! The CLI. Deliberately thin — argv in, one call into the library, bytes out
//! — following twig's own "modest, clean CLI" principle (`DESIGN.md`): plain
//! stdout/stderr, no logging framework, no colour machinery.

use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use twig_export::{Extensions, Rendered, config::Config, format_from_path, format_named, render};

#[derive(Parser)]
#[command(
    name = "twig-export",
    about = "Render a Twig document to Typst or PDF.",
    version
)]
struct Args {
    /// Input document (.dj/.djot, .md/.markdown, .xml, .html/.htm).
    #[arg(required_unless_present_any = ["install", "adapt", "check"])]
    input: Option<PathBuf>,

    /// Output file. The extension selects the backend: `.typ`, `.pdf`, or
    /// `.html`/`.htm` (experimental — see `html.rs`). Omit to write Typst to
    /// stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Style config (.toml/.yaml/.json/.figl). Without one, built-in defaults
    /// render a plain but complete document.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override input-format detection.
    #[arg(short = 'i', long)]
    input_format: Option<String>,

    /// Enable generic directives (`:name`, `::name`, `:::name`) — the syntax
    /// most style rules key off. Mirrors `twig --directives`.
    #[arg(long)]
    directives: bool,

    /// Enable `$…$` / `$$…$$` math.
    #[arg(long)]
    math: bool,

    /// Parse recognised raw HTML into semantic nodes, so an `<img>` becomes a
    /// real image instead of an opaque raw block.
    #[arg(long)]
    html_elements: bool,

    /// Report which rules matched what, and which metadata bindings never
    /// fired. The answer to "why did my styling do nothing?".
    #[arg(long)]
    explain: bool,

    /// A folder of Typst packages, laid out as Typst's own
    /// (`namespace/name/version/`), to find templates in. Repeatable; the
    /// first is where `--install` and `--adapt` write.
    #[arg(long)]
    packages: Vec<PathBuf>,

    /// Fetch a Typst Universe package (`@preview/name:1.0.0`), and every
    /// package it imports, into the first `--packages` folder.
    #[arg(long, value_name = "SPEC")]
    install: Option<String>,

    /// Write a twig-export template over a Typst Universe template
    /// (`@preview/name:1.0.0`), installed in the first `--packages` folder,
    /// as `@adapted/name:1.0.0` beside it, and check it.
    #[arg(long, value_name = "SPEC")]
    adapt: Option<String>,

    /// Check that a template (a built-in's name, or a spec) sets a sample
    /// document in each of its layouts.
    #[arg(long, value_name = "TEMPLATE")]
    check: Option<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("twig-export: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if let Some(spec) = &args.install {
        install(spec, &args.packages)?;
    }
    if let Some(spec) = &args.adapt {
        adapt(spec, &args.packages)?;
    }
    if let Some(template) = &args.check {
        check(template, &args.packages)?;
    }
    let Some(input) = &args.input else {
        return Ok(());
    };

    let source =
        std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;

    let format = match &args.input_format {
        Some(name) => parse_format(name)?,
        None => format_from_path(input)
            .with_context(|| format!("cannot infer a format from {} — pass -i", input.display()))?,
    };

    let (cfg, warnings) = match &args.config {
        Some(p) => Config::load(p)?,
        None => (Config::default(), Vec::new()),
    };

    // Warnings print unconditionally, not only under --explain: an open
    // vocabulary's unknown value is passed through to Typst, so the user needs
    // to know it was not recognised even when the render succeeds.
    if !warnings.is_empty() {
        let mut err = std::io::stderr().lock();
        for w in &warnings {
            let _ = writeln!(err, "twig-export: warning: {w}");
        }
    }

    let extensions = Extensions {
        directives: args.directives,
        math: args.math,
        html_elements: args.html_elements,
    };

    let rendered = render(&source, format, &cfg, extensions)?;

    if args.explain {
        explain(&cfg, &rendered);
    }

    match &args.output {
        // Typst written out for typst-cli carries a built-in template's
        // source: typst-cli serves no `twig` namespace.
        None => {
            print!("{}", twig_export::templates::inline(&rendered.typst));
            Ok(())
        }
        Some(path) => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            match ext.as_str() {
                "typ" => std::fs::write(path, twig_export::templates::inline(&rendered.typst))
                    .with_context(|| format!("writing {}", path.display())),
                "pdf" => write_pdf(path, &rendered.typst, &args.packages),
                "html" | "htm" => write_html(path, &rendered.typst),
                other => bail!(
                    "unknown output extension {other:?} (expected .typ, .pdf, .html, or .htm)"
                ),
            }
        }
    }
}

#[cfg(feature = "pdf")]
fn write_pdf(path: &std::path::Path, typ: &str, packages: &[PathBuf]) -> Result<()> {
    use twig_export::pdf::{Packages, Typesetter};
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    // As typst-cli does: a template that dates a document by today sets.
    let bytes = Typesetter::new()
        .with_packages(Packages::new(packages.iter().cloned()))
        .with_today(Some(today()))
        .pdf(typ, &cwd)?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(not(feature = "pdf"))]
fn write_pdf(_path: &std::path::Path, _typ: &str, _packages: &[PathBuf]) -> Result<()> {
    bail!(
        "this build has no PDF backend (built without the `pdf` feature).\n\
         Emit Typst with `-o out.typ` and compile it separately."
    )
}

#[cfg(feature = "html")]
fn write_html(path: &std::path::Path, typ: &str) -> Result<()> {
    let html = twig_export::html::compile(typ)?;
    std::fs::write(path, html).with_context(|| format!("writing {}", path.display()))
}

#[cfg(not(feature = "html"))]
fn write_html(_path: &std::path::Path, _typ: &str) -> Result<()> {
    bail!(
        "this build has no HTML backend (built without the `html` feature, which is off by \
         default — Typst's HTML export is experimental).\n\
         Emit Typst with `-o out.typ` and compile it separately."
    )
}

/// Today's date, in UTC, as (year, month, day).
#[cfg(feature = "pdf")]
fn today() -> (i32, u8, u8) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Howard Hinnant's days-to-civil.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
    (y, m, d)
}

/// The first `--packages` folder, where `--install` and `--adapt` write.
fn first(packages: &[PathBuf]) -> Result<&PathBuf> {
    packages
        .first()
        .context("name a folder of packages to write to, with --packages")
}

fn spec(s: &str) -> Result<twig_export::templates::Spec> {
    twig_export::templates::Spec::parse(s)
        .with_context(|| format!("{s:?} is not a package spec (@namespace/name:1.0.0)"))
}

#[cfg(feature = "network")]
fn install(s: &str, packages: &[PathBuf]) -> Result<()> {
    let downloader = typst_kit::downloader::SystemDownloader::new(concat!(
        "twig-export/",
        env!("CARGO_PKG_VERSION")
    ));
    let fetched = twig_export::install::install(&spec(s)?, first(packages)?, downloader)?;
    for spec in fetched {
        eprintln!("twig-export: installed {spec}");
    }
    Ok(())
}

#[cfg(not(feature = "network"))]
fn install(_s: &str, _packages: &[PathBuf]) -> Result<()> {
    bail!("this build cannot fetch packages (built without the `network` feature)")
}

fn adapt(s: &str, packages: &[PathBuf]) -> Result<()> {
    let spec = spec(s)?;
    let dir = first(packages)?;
    let adapter = twig_export::adapt::adapt(&spec, &spec.dir_in(dir))?;
    adapter.write(dir)?;
    eprintln!("twig-export: wrote {}", adapter.spec);
    for (arg, keys) in &adapter.args {
        eprintln!("  {arg}: from {}", keys.join(" or "));
    }
    check(&adapter.spec.to_string(), packages)
}

#[cfg(feature = "pdf")]
fn check(template: &str, packages: &[PathBuf]) -> Result<()> {
    use twig_export::pdf::{Packages, Typesetter};
    use twig_export::templates::{Manifest, builtin};
    let manifest = match builtin(template) {
        Some(b) => b.manifest(),
        None => {
            let spec = spec(template)?;
            let dir = packages
                .iter()
                .map(|d| spec.dir_in(d))
                .find(|d| d.is_dir())
                .with_context(|| format!("{template} is in none of the --packages folders"))?;
            let toml = std::fs::read_to_string(dir.join("typst.toml"))?;
            Manifest::parse(&toml)?
        }
    };
    let twig = manifest.twig().with_context(|| {
        format!("{template} has no [tool.twig]: it is not a twig-export template")
    })?;
    Typesetter::new()
        .with_packages(Packages::new(packages.iter().cloned()))
        .check(template, &twig.layouts)?;
    eprintln!(
        "twig-export: {template} sets, {}",
        twig.layouts.join(" and ")
    );
    Ok(())
}

#[cfg(not(feature = "pdf"))]
fn check(_template: &str, _packages: &[PathBuf]) -> Result<()> {
    bail!("this build has no Typst (built without the `pdf` feature)")
}

fn parse_format(name: &str) -> Result<twig::Format> {
    match format_named(name) {
        Some(format) => Ok(format),
        None => bail!(
            "unknown input format {:?} (djot, markdown, xml, html)",
            name.to_ascii_lowercase()
        ),
    }
}

fn explain(cfg: &Config, r: &Rendered) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "rules:");
    for (rule, n) in cfg.rules.iter().zip(&r.match_counts) {
        let note = if *n == 0 { "  <- matched nothing" } else { "" };
        let _ = writeln!(err, "  {:>4}  {}{note}", n, rule.select);
    }
    if !r.unfired_bindings.is_empty() {
        let _ = writeln!(err, "metadata bindings that never fired:");
        for b in &r.unfired_bindings {
            let _ = writeln!(err, "  {b}");
        }
    }
}
