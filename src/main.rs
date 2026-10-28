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
    input: PathBuf,

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
}

fn main() {
    if let Err(e) = run() {
        eprintln!("twig-export: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    let source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("reading {}", args.input.display()))?;

    let format = match &args.input_format {
        Some(name) => parse_format(name)?,
        None => format_from_path(&args.input).with_context(|| {
            format!(
                "cannot infer a format from {} — pass -i",
                args.input.display()
            )
        })?,
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
                "pdf" => write_pdf(path, &rendered.typst),
                "html" | "htm" => write_html(path, &rendered.typst),
                other => bail!(
                    "unknown output extension {other:?} (expected .typ, .pdf, .html, or .htm)"
                ),
            }
        }
    }
}

#[cfg(feature = "pdf")]
fn write_pdf(path: &std::path::Path, typ: &str) -> Result<()> {
    let bytes = twig_export::pdf::compile(typ)?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(not(feature = "pdf"))]
fn write_pdf(_path: &std::path::Path, _typ: &str) -> Result<()> {
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
